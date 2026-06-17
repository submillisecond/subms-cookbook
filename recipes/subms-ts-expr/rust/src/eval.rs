//! The evaluator. [`eval`] walks a [`TsExpr`] tree against a [`TsDataFrame`],
//! producing one typed [`TsArray`] aligned to the frame's union-of-timestamps
//! row axis. Evaluation is column-at-a-time: each node materialises its whole
//! array, then the parent combines child arrays elementwise. That is the
//! analytical-front model (push throughput), not the per-op tick-loop model.
//!
//! The type rules live here, not in the IR. A `Col` takes its column's
//! `TsDataType`; a `Lit` takes its literal's type; arithmetic promotes
//! `I64`/`F64` to `F64`; comparison yields `Bool`; an `Agg` resolves to the
//! result type its reduction defines (`Sum`/`Mean` -> `F64`, `Count` -> `I64`,
//! `Min`/`Max` -> the operand's type). A mismatch the rules cannot reconcile is
//! a [`TsExprError::TypeMismatch`], not a silent coercion.

use std::collections::BTreeMap;

use subms_ts::{TsDataFrame, TsDataType, TsValue};

use crate::array::TsArray;
use crate::expr::{TsAggOp, TsBinaryOp, TsCmpOp, TsExpr, TsUnaryOp};

/// Errors the evaluator can raise. Pure-compute errors only - a structurally
/// missing cell is a validity bit, not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsExprError {
    /// `Col(name)` referenced a column the frame does not hold.
    UnknownColumn(String),
    /// An operator received operand types its rules cannot reconcile (e.g.
    /// arithmetic on a `Str`, a `When` whose arms disagree, a numeric compare
    /// across a numeric and a string).
    TypeMismatch(String),
    /// `eval_scalar` was asked for a scalar from a non-`Agg` top-level
    /// expression (which is per-row, not a single reduced value).
    NotScalar,
}

impl std::fmt::Display for TsExprError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsExprError::UnknownColumn(name) => write!(f, "unknown column: {name}"),
            TsExprError::TypeMismatch(why) => write!(f, "type mismatch: {why}"),
            TsExprError::NotScalar => {
                write!(f, "eval_scalar requires a top-level Agg expression")
            }
        }
    }
}

impl std::error::Error for TsExprError {}

/// The frame's row axis: the column types plus, per column, a dense
/// `Vec<Option<TsValue>>` over the union-of-timestamps rows. Materialised once
/// per `eval` so a deep tree pays the aligned-walk cost a single time, not per
/// `Col` node.
struct RowAxis {
    nrows: usize,
    types: BTreeMap<String, TsDataType>,
    cells: BTreeMap<String, Vec<Option<TsValue>>>,
}

impl RowAxis {
    fn build(frame: &TsDataFrame) -> RowAxis {
        let order: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
        let mut types = BTreeMap::new();
        for name in &order {
            if let Some(col) = frame.column(name) {
                types.insert(name.clone(), col.data_type());
            }
        }
        let mut cells: BTreeMap<String, Vec<Option<TsValue>>> =
            order.iter().map(|n| (n.clone(), Vec::new())).collect();

        let mut nrows = 0;
        for (_ts, row) in frame.aligned() {
            for (name, cell) in order.iter().zip(row) {
                cells.get_mut(name).unwrap().push(cell);
            }
            nrows += 1;
        }
        RowAxis {
            nrows,
            types,
            cells,
        }
    }

    fn column(&self, name: &str) -> Option<TsArray> {
        let ty = *self.types.get(name)?;
        let cells = self.cells.get(name)?;
        Some(column_to_array(ty, cells))
    }
}

// Project a column's per-row Option<TsValue> cells onto a typed TsArray of the
// column's declared dtype. A cell whose boxed value does not match the dtype is
// treated as null (a column never mixes types in practice, so this is defensive).
fn column_to_array(ty: TsDataType, cells: &[Option<TsValue>]) -> TsArray {
    let n = cells.len();
    match ty {
        TsDataType::F64 | TsDataType::Value => {
            let mut values = vec![0.0; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(v) = c.as_ref().and_then(as_f64) {
                    values[i] = v;
                    valid[i] = true;
                }
            }
            TsArray::F64 { values, valid }
        }
        TsDataType::I64 => {
            let mut values = vec![0i64; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(TsValue::I64(v)) = c {
                    values[i] = *v;
                    valid[i] = true;
                }
            }
            TsArray::I64 { values, valid }
        }
        TsDataType::Bool => {
            let mut values = vec![false; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(TsValue::Bool(v)) = c {
                    values[i] = *v;
                    valid[i] = true;
                }
            }
            TsArray::Bool { values, valid }
        }
        TsDataType::Str => {
            let mut values = vec![String::new(); n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(TsValue::Str(v)) = c {
                    values[i] = v.clone();
                    valid[i] = true;
                }
            }
            TsArray::Str { values, valid }
        }
    }
}

fn as_f64(v: &TsValue) -> Option<f64> {
    match v {
        TsValue::F64(x) => Some(*x),
        TsValue::I64(x) => Some(*x as f64),
        _ => None,
    }
}

/// Evaluate `expr` over `frame`, returning a typed [`TsArray`] aligned to the
/// frame's union-of-timestamps row axis.
pub fn eval(expr: &TsExpr, frame: &TsDataFrame) -> Result<TsArray, TsExprError> {
    let axis = RowAxis::build(frame);
    eval_node(expr, &axis)
}

/// Convenience for a top-level [`TsExpr::Agg`]: evaluate and return the single
/// scalar [`TsValue`]. This is the operators' per-group / per-partition entry
/// point. Errors with [`TsExprError::NotScalar`] for any other top-level shape.
/// On an empty frame the reduction's empty value is returned (Count -> 0,
/// Sum -> 0.0, Mean -> NaN, Min/Max -> Null).
pub fn eval_scalar(expr: &TsExpr, frame: &TsDataFrame) -> Result<TsValue, TsExprError> {
    match expr {
        TsExpr::Agg(_, _) => {
            let axis = RowAxis::build(frame);
            let arr = eval_node(expr, &axis)?;
            if arr.is_empty() {
                // An empty frame yields a zero-row broadcast, so recover the
                // reduction's defined empty scalar directly.
                let TsExpr::Agg(op, operand) = expr else {
                    unreachable!()
                };
                let inner_ty = result_type(operand, &axis)?;
                empty_agg_scalar(*op, inner_ty)
            } else {
                Ok(arr.get(0).unwrap_or(TsValue::Null))
            }
        }
        _ => Err(TsExprError::NotScalar),
    }
}

fn eval_node(expr: &TsExpr, axis: &RowAxis) -> Result<TsArray, TsExprError> {
    match expr {
        TsExpr::Col(name) => axis
            .column(name)
            .ok_or_else(|| TsExprError::UnknownColumn(name.clone())),
        TsExpr::Lit(v) => broadcast_lit(v, axis.nrows),
        TsExpr::Unary(op, operand) => {
            let c = eval_node(operand, axis)?;
            eval_unary(*op, &c)
        }
        TsExpr::Binary(op, lhs, rhs) => {
            let l = eval_node(lhs, axis)?;
            let r = eval_node(rhs, axis)?;
            eval_binary(*op, &l, &r)
        }
        TsExpr::Compare(op, lhs, rhs) => {
            let l = eval_node(lhs, axis)?;
            let r = eval_node(rhs, axis)?;
            eval_compare(*op, &l, &r)
        }
        TsExpr::When {
            cond,
            then,
            otherwise,
        } => {
            let c = eval_node(cond, axis)?;
            let t = eval_node(then, axis)?;
            let f = eval_node(otherwise, axis)?;
            eval_when(&c, &t, &f)
        }
        TsExpr::Agg(op, operand) => {
            let c = eval_node(operand, axis)?;
            let scalar = reduce(*op, &c)?;
            Ok(broadcast_scalar(&scalar, axis.nrows))
        }
    }
}

// Resolve the element type an expression yields without materialising it - used
// to recover the empty-frame Agg scalar's type.
fn result_type(expr: &TsExpr, axis: &RowAxis) -> Result<TsDataType, TsExprError> {
    match expr {
        TsExpr::Col(name) => axis
            .types
            .get(name)
            .copied()
            .map(|t| match t {
                TsDataType::Value => TsDataType::F64,
                other => other,
            })
            .ok_or_else(|| TsExprError::UnknownColumn(name.clone())),
        TsExpr::Lit(v) => lit_type(v),
        TsExpr::Unary(_, operand) => result_type(operand, axis),
        TsExpr::Binary(_, lhs, rhs) => {
            let l = result_type(lhs, axis)?;
            let r = result_type(rhs, axis)?;
            numeric_result(l, r)
        }
        TsExpr::Compare(_, _, _) => Ok(TsDataType::Bool),
        TsExpr::When { then, .. } => result_type(then, axis),
        TsExpr::Agg(op, operand) => agg_result_type(*op, result_type(operand, axis)?),
    }
}

fn lit_type(v: &TsValue) -> Result<TsDataType, TsExprError> {
    match v {
        TsValue::F64(_) => Ok(TsDataType::F64),
        TsValue::I64(_) => Ok(TsDataType::I64),
        TsValue::Bool(_) => Ok(TsDataType::Bool),
        TsValue::Str(_) => Ok(TsDataType::Str),
        other => Err(TsExprError::TypeMismatch(format!(
            "unsupported literal type: {other:?}"
        ))),
    }
}

fn broadcast_lit(v: &TsValue, n: usize) -> Result<TsArray, TsExprError> {
    Ok(match v {
        TsValue::F64(x) => TsArray::F64 {
            values: vec![*x; n],
            valid: vec![true; n],
        },
        TsValue::I64(x) => TsArray::I64 {
            values: vec![*x; n],
            valid: vec![true; n],
        },
        TsValue::Bool(x) => TsArray::Bool {
            values: vec![*x; n],
            valid: vec![true; n],
        },
        TsValue::Str(s) => TsArray::Str {
            values: vec![s.clone(); n],
            valid: vec![true; n],
        },
        other => {
            return Err(TsExprError::TypeMismatch(format!(
                "unsupported literal type: {other:?}"
            )));
        }
    })
}

fn broadcast_scalar(v: &TsValue, n: usize) -> TsArray {
    match v {
        TsValue::F64(x) => TsArray::F64 {
            values: vec![*x; n],
            valid: vec![true; n],
        },
        TsValue::I64(x) => TsArray::I64 {
            values: vec![*x; n],
            valid: vec![true; n],
        },
        TsValue::Bool(x) => TsArray::Bool {
            values: vec![*x; n],
            valid: vec![true; n],
        },
        TsValue::Str(s) => TsArray::Str {
            values: vec![s.clone(); n],
            valid: vec![true; n],
        },
        // A null scalar (e.g. Min over an all-null operand) broadcasts as an
        // all-null F64 column; the type is irrelevant since every cell is null.
        _ => TsArray::F64 {
            values: vec![0.0; n],
            valid: vec![false; n],
        },
    }
}

fn numeric_result(l: TsDataType, r: TsDataType) -> Result<TsDataType, TsExprError> {
    match (l, r) {
        (TsDataType::I64, TsDataType::I64) => Ok(TsDataType::I64),
        (TsDataType::F64, TsDataType::F64)
        | (TsDataType::F64, TsDataType::I64)
        | (TsDataType::I64, TsDataType::F64) => Ok(TsDataType::F64),
        _ => Err(TsExprError::TypeMismatch(format!(
            "arithmetic requires numeric operands, got {l:?} and {r:?}"
        ))),
    }
}

fn eval_unary(op: TsUnaryOp, c: &TsArray) -> Result<TsArray, TsExprError> {
    match c {
        TsArray::F64 { values, valid } => {
            let out = values
                .iter()
                .map(|&v| match op {
                    TsUnaryOp::Neg => -v,
                    TsUnaryOp::Abs => v.abs(),
                })
                .collect();
            Ok(TsArray::F64 {
                values: out,
                valid: valid.clone(),
            })
        }
        TsArray::I64 { values, valid } => {
            let out = values
                .iter()
                .map(|&v| match op {
                    TsUnaryOp::Neg => v.wrapping_neg(),
                    TsUnaryOp::Abs => v.wrapping_abs(),
                })
                .collect();
            Ok(TsArray::I64 {
                values: out,
                valid: valid.clone(),
            })
        }
        other => Err(TsExprError::TypeMismatch(format!(
            "unary {op:?} requires a numeric operand, got {:?}",
            other.data_type()
        ))),
    }
}

fn eval_binary(op: TsBinaryOp, l: &TsArray, r: &TsArray) -> Result<TsArray, TsExprError> {
    match (l, r) {
        (TsArray::I64 { .. }, TsArray::I64 { .. }) => {
            let (lv, lok) = l.as_i64().unwrap();
            let (rv, rok) = r.as_i64().unwrap();
            Ok(binary_i64(op, lv, lok, rv, rok))
        }
        (TsArray::F64 { .. }, _) | (_, TsArray::F64 { .. })
            if is_numeric(l) && is_numeric(r) =>
        {
            let (lv, lok) = to_f64(l);
            let (rv, rok) = to_f64(r);
            Ok(binary_f64(op, &lv, &lok, &rv, &rok))
        }
        _ => Err(TsExprError::TypeMismatch(format!(
            "arithmetic {op:?} requires numeric operands, got {:?} and {:?}",
            l.data_type(),
            r.data_type()
        ))),
    }
}

fn is_numeric(a: &TsArray) -> bool {
    matches!(a, TsArray::F64 { .. } | TsArray::I64 { .. })
}

// Widen a numeric array to (f64 values, valid) for mixed-type arithmetic.
fn to_f64(a: &TsArray) -> (Vec<f64>, Vec<bool>) {
    match a {
        TsArray::F64 { values, valid } => (values.clone(), valid.clone()),
        TsArray::I64 { values, valid } => {
            (values.iter().map(|&v| v as f64).collect(), valid.clone())
        }
        _ => unreachable!("to_f64 on a non-numeric array"),
    }
}

fn binary_i64(op: TsBinaryOp, lv: &[i64], lok: &[bool], rv: &[i64], rok: &[bool]) -> TsArray {
    let n = lv.len();
    let mut values = vec![0i64; n];
    let mut valid = vec![false; n];
    for i in 0..n {
        if !(lok[i] && rok[i]) {
            continue;
        }
        let (a, b) = (lv[i], rv[i]);
        match op {
            TsBinaryOp::Add => set(&mut values, &mut valid, i, a.wrapping_add(b)),
            TsBinaryOp::Sub => set(&mut values, &mut valid, i, a.wrapping_sub(b)),
            TsBinaryOp::Mul => set(&mut values, &mut valid, i, a.wrapping_mul(b)),
            TsBinaryOp::Div => {
                if b == 0 {
                    // a zero divisor is a missing cell, not a panic.
                    continue;
                }
                set(&mut values, &mut valid, i, a.wrapping_div(b));
            }
        }
    }
    TsArray::I64 { values, valid }
}

fn binary_f64(op: TsBinaryOp, lv: &[f64], lok: &[bool], rv: &[f64], rok: &[bool]) -> TsArray {
    let n = lv.len();
    let mut values = vec![0.0; n];
    let mut valid = vec![false; n];
    for i in 0..n {
        if !(lok[i] && rok[i]) {
            continue;
        }
        let (a, b) = (lv[i], rv[i]);
        match op {
            TsBinaryOp::Add => set(&mut values, &mut valid, i, a + b),
            TsBinaryOp::Sub => set(&mut values, &mut valid, i, a - b),
            TsBinaryOp::Mul => set(&mut values, &mut valid, i, a * b),
            TsBinaryOp::Div => {
                if b == 0.0 {
                    // divide-by-zero is a missing cell, not a NaN/inf that
                    // poisons every downstream reduction silently.
                    continue;
                }
                set(&mut values, &mut valid, i, a / b);
            }
        }
    }
    TsArray::F64 { values, valid }
}

fn set<T>(values: &mut [T], valid: &mut [bool], i: usize, v: T) {
    values[i] = v;
    valid[i] = true;
}

fn eval_compare(op: TsCmpOp, l: &TsArray, r: &TsArray) -> Result<TsArray, TsExprError> {
    let n = l.len();
    let mut values = vec![false; n];
    let mut valid = vec![false; n];
    match (l, r) {
        _ if is_numeric(l) && is_numeric(r) => {
            let (lv, lok) = to_f64(l);
            let (rv, rok) = to_f64(r);
            for i in 0..n {
                if lok[i] && rok[i] {
                    set(&mut values, &mut valid, i, cmp_ord(op, lv[i], rv[i]));
                }
            }
        }
        (TsArray::Str { values: lv, valid: lok }, TsArray::Str { values: rv, valid: rok }) => {
            for i in 0..n {
                if lok[i] && rok[i] {
                    set(&mut values, &mut valid, i, cmp_ord(op, &lv[i], &rv[i]));
                }
            }
        }
        (TsArray::Bool { values: lv, valid: lok }, TsArray::Bool { values: rv, valid: rok }) => {
            for i in 0..n {
                if !(lok[i] && rok[i]) {
                    continue;
                }
                let res = match op {
                    TsCmpOp::Eq => lv[i] == rv[i],
                    TsCmpOp::Ne => lv[i] != rv[i],
                    _ => {
                        return Err(TsExprError::TypeMismatch(
                            "bool compare supports only eq / ne".to_string(),
                        ));
                    }
                };
                set(&mut values, &mut valid, i, res);
            }
        }
        _ => {
            return Err(TsExprError::TypeMismatch(format!(
                "compare {op:?} requires same-type operands, got {:?} and {:?}",
                l.data_type(),
                r.data_type()
            )));
        }
    }
    Ok(TsArray::Bool { values, valid })
}

fn cmp_ord<T: PartialOrd>(op: TsCmpOp, a: T, b: T) -> bool {
    match op {
        TsCmpOp::Lt => a < b,
        TsCmpOp::Le => a <= b,
        TsCmpOp::Eq => a == b,
        TsCmpOp::Ne => a != b,
        TsCmpOp::Ge => a >= b,
        TsCmpOp::Gt => a > b,
    }
}

fn eval_when(cond: &TsArray, then: &TsArray, otherwise: &TsArray) -> Result<TsArray, TsExprError> {
    let (mask, mask_valid) = match cond {
        TsArray::Bool { values, valid } => (values, valid),
        other => {
            return Err(TsExprError::TypeMismatch(format!(
                "when condition must be Bool, got {:?}",
                other.data_type()
            )));
        }
    };
    if then.data_type() != otherwise.data_type() {
        return Err(TsExprError::TypeMismatch(format!(
            "when branches disagree: then is {:?}, otherwise is {:?}",
            then.data_type(),
            otherwise.data_type()
        )));
    }
    let n = cond.len();
    Ok(match then {
        TsArray::F64 { .. } => {
            let (tv, tok) = then.as_f64().unwrap();
            let (fv, fok) = otherwise.as_f64().unwrap();
            select(n, mask, mask_valid, tv, tok, fv, fok, 0.0, |values, valid| {
                TsArray::F64 { values, valid }
            })
        }
        TsArray::I64 { .. } => {
            let (tv, tok) = then.as_i64().unwrap();
            let (fv, fok) = otherwise.as_i64().unwrap();
            select(n, mask, mask_valid, tv, tok, fv, fok, 0, |values, valid| {
                TsArray::I64 { values, valid }
            })
        }
        TsArray::Bool { .. } => {
            let (tv, tok) = then.as_bool().unwrap();
            let (fv, fok) = otherwise.as_bool().unwrap();
            select(n, mask, mask_valid, tv, tok, fv, fok, false, |values, valid| {
                TsArray::Bool { values, valid }
            })
        }
        TsArray::Str { .. } => {
            let (tv, tok) = then.as_str().unwrap();
            let (fv, fok) = otherwise.as_str().unwrap();
            let mut values = vec![String::new(); n];
            let mut valid = vec![false; n];
            for i in 0..n {
                if !mask_valid[i] {
                    continue;
                }
                let (src, ok) = if mask[i] { (tv, tok) } else { (fv, fok) };
                if ok[i] {
                    values[i] = src[i].clone();
                    valid[i] = true;
                }
            }
            TsArray::Str { values, valid }
        }
    })
}

#[allow(clippy::too_many_arguments)]
fn select<T: Copy, F: FnOnce(Vec<T>, Vec<bool>) -> TsArray>(
    n: usize,
    mask: &[bool],
    mask_valid: &[bool],
    tv: &[T],
    tok: &[bool],
    fv: &[T],
    fok: &[bool],
    default: T,
    build: F,
) -> TsArray {
    let mut values = vec![default; n];
    let mut valid = vec![false; n];
    for i in 0..n {
        if !mask_valid[i] {
            continue;
        }
        let (src, ok) = if mask[i] { (tv, tok) } else { (fv, fok) };
        if ok[i] {
            values[i] = src[i];
            valid[i] = true;
        }
    }
    build(values, valid)
}

fn agg_result_type(op: TsAggOp, operand: TsDataType) -> Result<TsDataType, TsExprError> {
    match op {
        TsAggOp::Sum | TsAggOp::Mean => Ok(TsDataType::F64),
        TsAggOp::Count => Ok(TsDataType::I64),
        TsAggOp::Min | TsAggOp::Max => Ok(operand),
    }
}

// Reduce over the VALID cells only, returning the scalar TsValue the Agg
// broadcasts. Sum / Mean -> F64; Count -> I64; Min / Max -> the operand's type.
fn reduce(op: TsAggOp, c: &TsArray) -> Result<TsValue, TsExprError> {
    if op == TsAggOp::Count {
        return Ok(TsValue::I64(c.valid_count() as i64));
    }
    match op {
        TsAggOp::Sum | TsAggOp::Mean => {
            let vals = numeric_valid(c)?;
            let scalar = match op {
                TsAggOp::Sum => vals.iter().sum(),
                TsAggOp::Mean => {
                    if vals.is_empty() {
                        f64::NAN
                    } else {
                        vals.iter().sum::<f64>() / vals.len() as f64
                    }
                }
                _ => unreachable!(),
            };
            Ok(TsValue::F64(scalar))
        }
        TsAggOp::Min | TsAggOp::Max => min_max(op, c),
        TsAggOp::Count => unreachable!(),
    }
}

fn numeric_valid(c: &TsArray) -> Result<Vec<f64>, TsExprError> {
    match c {
        TsArray::F64 { values, valid } => Ok(values
            .iter()
            .zip(valid)
            .filter_map(|(&v, &ok)| ok.then_some(v))
            .collect()),
        TsArray::I64 { values, valid } => Ok(values
            .iter()
            .zip(valid)
            .filter_map(|(&v, &ok)| ok.then_some(v as f64))
            .collect()),
        other => Err(TsExprError::TypeMismatch(format!(
            "sum / mean require a numeric operand, got {:?}",
            other.data_type()
        ))),
    }
}

fn min_max(op: TsAggOp, c: &TsArray) -> Result<TsValue, TsExprError> {
    let pick_first = op == TsAggOp::Min;
    match c {
        TsArray::F64 { values, valid } => {
            let acc = fold_ord(values, valid, pick_first);
            Ok(acc.map(TsValue::F64).unwrap_or(TsValue::Null))
        }
        TsArray::I64 { values, valid } => {
            let acc = fold_ord(values, valid, pick_first);
            Ok(acc.map(TsValue::I64).unwrap_or(TsValue::Null))
        }
        TsArray::Str { values, valid } => {
            let mut acc: Option<&String> = None;
            for (v, &ok) in values.iter().zip(valid) {
                if ok && (acc.is_none() || keep(pick_first, v < acc.unwrap())) {
                    acc = Some(v);
                }
            }
            Ok(acc.map(|s| TsValue::Str(s.clone())).unwrap_or(TsValue::Null))
        }
        TsArray::Bool { .. } => Err(TsExprError::TypeMismatch(
            "min / max are not defined over Bool".to_string(),
        )),
    }
}

fn fold_ord<T: Copy + PartialOrd>(values: &[T], valid: &[bool], pick_first: bool) -> Option<T> {
    let mut acc: Option<T> = None;
    for (&v, &ok) in values.iter().zip(valid) {
        if ok && (acc.is_none() || keep(pick_first, v < acc.unwrap())) {
            acc = Some(v);
        }
    }
    acc
}

fn keep(pick_first: bool, less: bool) -> bool {
    if pick_first { less } else { !less }
}

fn empty_agg_scalar(op: TsAggOp, operand: TsDataType) -> Result<TsValue, TsExprError> {
    let _ = operand;
    Ok(match op {
        TsAggOp::Count => TsValue::I64(0),
        TsAggOp::Sum => TsValue::F64(0.0),
        TsAggOp::Mean => TsValue::F64(f64::NAN),
        TsAggOp::Min | TsAggOp::Max => TsValue::Null,
    })
}
