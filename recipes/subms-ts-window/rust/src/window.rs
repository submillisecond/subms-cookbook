//! The window-function engine. Every function partitions a [`TsDataFrame`]'s
//! aligned rows by a TUPLE of TYPED key cells, orders the rows inside each
//! partition by an order-by column (default: the frame's union-of-timestamps
//! row order), then runs a per-partition transform whose results are scattered
//! back to the original row positions. The output is a typed [`TsArray`] the
//! same length as the frame's aligned row axis - the same shape
//! `subms-ts-expr`'s `eval` produces, so a window output composes straight back
//! into further expression evaluation as a derived column.
//!
//! ## Row model
//!
//! A frame is a bag of named, per-column-typed series. We materialise its
//! union-of-timestamps row axis once (via `frame.aligned()`), exactly as the
//! evaluator does, so a row is a tuple of `Option<TsValue>` cells, one per
//! column. The partition key of a row is the tuple of the partition columns'
//! cells AT that row, and the key is TYPED: a `Str` symbol column keys directly
//! on the string, an `I64` venue id on the integer, an `F64` on its bit pattern,
//! a `Bool` on the flag. Partitioning by a symbol column is the headline case.
//!
//! ## Validity model
//!
//! Window outputs carry a validity bitmap for undefined cells, the same DERIVED
//! -null surface the evaluator uses. A `lag(1)` at a partition's first row has
//! no predecessor, so that cell is invalid, not zero. A running reduction over a
//! partition whose input cell is itself invalid skips that cell (the running
//! state carries forward; the output cell is invalid only until at least one
//! valid input has been folded in). This is distinct from `TsSeries`'
//! no-null-on-ingest invariant.
//!
//! ## Partition + order model
//!
//! Partitioning is by the tuple of partition-key cell values, in FIRST-SEEN
//! order, so the result is intuitive for the single-partition and stable cases
//! (partition order follows arrival, not a key sort). A row whose key tuple
//! contains a null cell lands in a dedicated null bucket keyed on which slots
//! are missing, so missing keys never collide with a present key and every row
//! still receives an output cell. Within a partition rows are ordered by the
//! order-by column with a STABLE sort, so equal order-by values keep their
//! arrival order - the SQL "order is a partial order, ties break on arrival"
//! contract. A null order-by value sorts before any present value (NULLS
//! FIRST), deterministically.
//!
//! ## Contract
//!
//! This recipe is THROUGHPUT-contracted, not per-op sub-ms. A window pass is an
//! analytical-front operation (partition, sort, scan), not a tick-loop op. The
//! bench measures whole-frame window passes over a partitioned 4,096-row frame
//! and asserts only a generous no-pathological-stall guard, not a tight sub-ms
//! p99. The honest number to read is throughput in `perf/rust.json`.

use std::collections::HashMap;

use subms_ts::{TsDataFrame, TsValue};
use subms_ts_expr::{TsArray, TsExpr, TsExprError, eval_scalar};

/// Errors the window engine can raise. These are structural (a referenced
/// column is missing, or a numeric reduction was asked of a non-numeric
/// column); a row whose key or input is null is handled by the validity model,
/// not an error.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsWindowError {
    /// A referenced column - the value column, an order-by column, or a
    /// partition key - is not a column the frame holds.
    UnknownColumn(String),
    /// A running reduction (`cumsum` / `cumprod` / `cummin` / `cummax`) was
    /// asked of a column whose type is not numeric (`F64` / `I64`).
    NotNumeric(String),
    /// `over` was handed a non-`Agg` top-level expression (which would be
    /// per-row, not a single value per partition).
    NotAnAggregation,
}

impl std::fmt::Display for TsWindowError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsWindowError::UnknownColumn(name) => write!(f, "unknown column: {name}"),
            TsWindowError::NotNumeric(name) => {
                write!(f, "column '{name}' is not numeric (F64/I64)")
            }
            TsWindowError::NotAnAggregation => {
                write!(f, "over requires a top-level Agg expression")
            }
        }
    }
}

impl std::error::Error for TsWindowError {}

impl From<TsExprError> for TsWindowError {
    fn from(e: TsExprError) -> Self {
        match e {
            TsExprError::UnknownColumn(name) => TsWindowError::UnknownColumn(name),
            TsExprError::TypeMismatch(why) => TsWindowError::NotNumeric(why),
            TsExprError::NotScalar => TsWindowError::NotAnAggregation,
        }
    }
}

/// A typed, hashable partition-key cell. An `F64` keys on its bit pattern (with
/// `-0.0` normalised to `0.0`) so equal values land in one partition; a stored
/// f64 is always finite (a series rejects non-finite on ingest). A `Null` cell
/// tags the slot index that was missing, so two distinct missing keys never
/// collide with each other or with a present key.
#[derive(Clone, PartialEq, Eq, Hash)]
enum KeyCell {
    I64(i64),
    F64(u64),
    Bool(bool),
    Str(String),
    Null(usize),
}

impl KeyCell {
    fn from_cell(cell: Option<&TsValue>, slot: usize) -> KeyCell {
        match cell {
            Some(TsValue::I64(x)) => KeyCell::I64(*x),
            Some(TsValue::F64(x)) => KeyCell::F64(f64_key_bits(*x)),
            Some(TsValue::Bool(b)) => KeyCell::Bool(*b),
            Some(TsValue::Str(s)) => KeyCell::Str(s.clone()),
            // A present-but-non-hashable cell (bytes / map / array / nested
            // null) behaves like a missing key: it cannot index a partition, so
            // it lands in the slot's null bucket.
            _ => KeyCell::Null(slot),
        }
    }
}

// Normalise -0.0 to +0.0 so the two zeros share a bit pattern (and a
// partition), matching value equality.
fn f64_key_bits(v: f64) -> u64 {
    let v = if v == 0.0 { 0.0 } else { v };
    v.to_bits()
}

/// The materialised row axis of a frame: column names + per-column dense
/// `Option<TsValue>` cells over the union-of-timestamps rows. Built once so the
/// partition pass and every per-partition scan share it.
struct RowAxis {
    names: Vec<String>,
    columns: Vec<Vec<Option<TsValue>>>,
    nrows: usize,
}

impl RowAxis {
    fn build(frame: &TsDataFrame) -> RowAxis {
        let names: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
        let mut columns: Vec<Vec<Option<TsValue>>> = vec![Vec::new(); names.len()];
        let mut nrows = 0;
        for (_ts, row) in frame.aligned() {
            for (i, cell) in row.into_iter().enumerate() {
                columns[i].push(cell);
            }
            nrows += 1;
        }
        RowAxis {
            names,
            columns,
            nrows,
        }
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    fn require(&self, name: &str) -> Result<usize, TsWindowError> {
        self.index_of(name)
            .ok_or_else(|| TsWindowError::UnknownColumn(name.to_string()))
    }
}

/// The shared partition + order plan: every row of the aligned axis grouped
/// into a partition (first-seen key order), each partition's row indices sorted
/// by the order-by column (stable on ties). Carries `nrows` so callers can size
/// their output.
struct Plan {
    nrows: usize,
    partitions: Vec<Vec<usize>>,
}

fn plan(
    axis: &RowAxis,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<Plan, TsWindowError> {
    let key_idx: Vec<usize> = partition_by
        .iter()
        .map(|name| axis.require(name))
        .collect::<Result<_, _>>()?;
    let order_idx = match order_by {
        Some(name) => Some(axis.require(name)?),
        None => None,
    };

    // Group rows into partitions in first-seen key order. The map keys on the
    // typed key tuple; partition order is arrival order, which keeps the
    // single-partition and stable cases intuitive.
    let mut index: HashMap<Vec<KeyCell>, usize> = HashMap::new();
    let mut partitions: Vec<Vec<usize>> = Vec::new();
    for row in 0..axis.nrows {
        let key: Vec<KeyCell> = key_idx
            .iter()
            .enumerate()
            .map(|(slot, &ci)| KeyCell::from_cell(axis.columns[ci][row].as_ref(), slot))
            .collect();
        let pidx = *index.entry(key).or_insert_with(|| {
            partitions.push(Vec::new());
            partitions.len() - 1
        });
        partitions[pidx].push(row);
    }

    if let Some(oi) = order_idx {
        for part in &mut partitions {
            part.sort_by(|&a, &b| {
                order_key(axis.columns[oi][a].as_ref())
                    .total_cmp(&order_key(axis.columns[oi][b].as_ref()))
            });
        }
    }

    Ok(Plan {
        nrows: axis.nrows,
        partitions,
    })
}

// A numeric order key. A null order-by value sorts first; a non-numeric present
// value also sorts first (its order is the row's arrival order, preserved by
// the stable sort). total_cmp gives a deterministic, total order.
fn order_key(cell: Option<&TsValue>) -> f64 {
    match cell {
        Some(TsValue::F64(x)) => *x,
        Some(TsValue::I64(x)) => *x as f64,
        Some(TsValue::Bool(b)) => *b as i64 as f64,
        _ => f64::NEG_INFINITY,
    }
}

// A numeric view of a cell (F64 / I64 widened to f64), for the running
// reductions. None for a missing or non-numeric cell.
fn numeric_of(cell: Option<&TsValue>) -> Option<f64> {
    match cell {
        Some(TsValue::F64(x)) => Some(*x),
        Some(TsValue::I64(x)) => Some(*x as f64),
        _ => None,
    }
}

// ---------- shift functions ----------

/// `lag(column, n)` within each partition: each row takes the value `n`
/// positions earlier in its partition's order. The first `n` rows of every
/// partition have no predecessor and are invalid. Works for ANY column type -
/// the result `TsArray` matches the input column's type.
pub fn lag(
    frame: &TsDataFrame,
    column: &str,
    n: usize,
    partition_by: &[&str],
) -> Result<TsArray, TsWindowError> {
    shift(frame, column, n as isize, partition_by, None)
}

/// `lead(column, n)` within each partition: each row takes the value `n`
/// positions later. The last `n` rows of every partition have no successor and
/// are invalid. Works for ANY column type.
pub fn lead(
    frame: &TsDataFrame,
    column: &str,
    n: usize,
    partition_by: &[&str],
) -> Result<TsArray, TsWindowError> {
    shift(frame, column, -(n as isize), partition_by, None)
}

fn shift(
    frame: &TsDataFrame,
    column: &str,
    offset: isize,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<TsArray, TsWindowError> {
    let axis = RowAxis::build(frame);
    let ci = axis.require(column)?;
    let plan = plan(&axis, partition_by, order_by)?;

    // The source row for each output row: which axis row supplies its value, or
    // None when the offset falls off the partition head/tail.
    let mut src = vec![None; plan.nrows];
    for part in &plan.partitions {
        let len = part.len() as isize;
        for (pos, &row) in part.iter().enumerate() {
            let from = pos as isize - offset;
            if (0..len).contains(&from) {
                src[row] = Some(part[from as usize]);
            }
        }
    }

    Ok(gather(&axis.columns[ci], &src))
}

// Build a typed array of the input column's type, taking each output row's value
// from its source row (or null where the source is None or itself a null cell).
fn gather(input: &[Option<TsValue>], src: &[Option<usize>]) -> TsArray {
    let kind = column_kind(input);
    let n = src.len();
    let cell = |i: usize| -> Option<&TsValue> { src[i].and_then(|r| input[r].as_ref()) };

    match kind {
        Kind::I64 => {
            let mut values = vec![0i64; n];
            let mut valid = vec![false; n];
            for (i, slot) in values.iter_mut().enumerate() {
                if let Some(TsValue::I64(x)) = cell(i) {
                    *slot = *x;
                    valid[i] = true;
                }
            }
            TsArray::I64 { values, valid }
        }
        Kind::Bool => {
            let mut values = vec![false; n];
            let mut valid = vec![false; n];
            for (i, slot) in values.iter_mut().enumerate() {
                if let Some(TsValue::Bool(x)) = cell(i) {
                    *slot = *x;
                    valid[i] = true;
                }
            }
            TsArray::Bool { values, valid }
        }
        Kind::Str => {
            let mut values = vec![String::new(); n];
            let mut valid = vec![false; n];
            for (i, slot) in values.iter_mut().enumerate() {
                if let Some(TsValue::Str(x)) = cell(i) {
                    *slot = x.clone();
                    valid[i] = true;
                }
            }
            TsArray::Str { values, valid }
        }
        Kind::F64 => {
            let mut values = vec![0.0f64; n];
            let mut valid = vec![false; n];
            for (i, slot) in values.iter_mut().enumerate() {
                if let Some(TsValue::F64(x)) = cell(i) {
                    *slot = *x;
                    valid[i] = true;
                }
            }
            TsArray::F64 { values, valid }
        }
    }
}

#[derive(Copy, Clone)]
enum Kind {
    F64,
    I64,
    Bool,
    Str,
}

// Pick a column's element type from its first present cell; an all-null column
// defaults to F64 (an empty typed array a downstream numeric op can still read).
fn column_kind(cells: &[Option<TsValue>]) -> Kind {
    cells
        .iter()
        .find_map(|c| match c {
            Some(TsValue::F64(_)) => Some(Kind::F64),
            Some(TsValue::I64(_)) => Some(Kind::I64),
            Some(TsValue::Bool(_)) => Some(Kind::Bool),
            Some(TsValue::Str(_)) => Some(Kind::Str),
            _ => None,
        })
        .unwrap_or(Kind::F64)
}

// ---------- numbering / ranking ----------

/// `row_number()` within each partition: 1..=k in order-by order. Always fully
/// valid - every row has a position. The result is an `I64` array.
pub fn row_number(
    frame: &TsDataFrame,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<TsArray, TsWindowError> {
    let axis = RowAxis::build(frame);
    let plan = plan(&axis, partition_by, order_by)?;
    let mut values = vec![0i64; plan.nrows];
    let mut valid = vec![false; plan.nrows];
    for part in &plan.partitions {
        for (pos, &row) in part.iter().enumerate() {
            values[row] = (pos + 1) as i64;
            valid[row] = true;
        }
    }
    Ok(TsArray::I64 { values, valid })
}

/// `rank()` within each partition: order-sensitive rank where ties share the
/// lowest rank and the next distinct value skips the gap (1, 1, 3, ...). Ranks
/// are by the order-by column. The result is an `I64` array.
pub fn rank(
    frame: &TsDataFrame,
    partition_by: &[&str],
    order_by: &str,
) -> Result<TsArray, TsWindowError> {
    ranked(frame, partition_by, order_by, false)
}

/// `dense_rank()` within each partition: like [`rank`] but consecutive distinct
/// values do not skip (1, 1, 2, ...). The result is an `I64` array.
pub fn dense_rank(
    frame: &TsDataFrame,
    partition_by: &[&str],
    order_by: &str,
) -> Result<TsArray, TsWindowError> {
    ranked(frame, partition_by, order_by, true)
}

fn ranked(
    frame: &TsDataFrame,
    partition_by: &[&str],
    order_by: &str,
    dense: bool,
) -> Result<TsArray, TsWindowError> {
    let axis = RowAxis::build(frame);
    let oi = axis.require(order_by)?;
    let plan = plan(&axis, partition_by, Some(order_by))?;
    let mut values = vec![0i64; plan.nrows];
    let mut valid = vec![false; plan.nrows];

    for part in &plan.partitions {
        let mut rank_value = 0i64;
        let mut prev: Option<f64> = None;
        for (idx, &row) in part.iter().enumerate() {
            let key = order_key(axis.columns[oi][row].as_ref());
            let is_new = match prev {
                Some(p) => key.total_cmp(&p) != std::cmp::Ordering::Equal,
                None => true,
            };
            if is_new {
                // The gap-skipping rank jumps to the 1-based position; the dense
                // rank advances by one.
                rank_value = if dense {
                    rank_value + 1
                } else {
                    (idx + 1) as i64
                };
            }
            values[row] = rank_value;
            valid[row] = true;
            prev = Some(key);
        }
    }
    Ok(TsArray::I64 { values, valid })
}

// ---------- running reductions ----------

#[derive(Copy, Clone)]
enum RunOp {
    Sum,
    Prod,
    Min,
    Max,
}

fn cumulative(
    frame: &TsDataFrame,
    column: &str,
    partition_by: &[&str],
    order_by: Option<&str>,
    op: RunOp,
) -> Result<TsArray, TsWindowError> {
    let axis = RowAxis::build(frame);
    let ci = axis.require(column)?;
    if !matches!(column_kind(&axis.columns[ci]), Kind::F64 | Kind::I64) {
        return Err(TsWindowError::NotNumeric(column.to_string()));
    }
    let plan = plan(&axis, partition_by, order_by)?;
    let mut values = vec![0.0f64; plan.nrows];
    let mut valid = vec![false; plan.nrows];

    for part in &plan.partitions {
        let mut acc: Option<f64> = None;
        for &row in part {
            if let Some(v) = numeric_of(axis.columns[ci][row].as_ref()) {
                acc = Some(match (acc, op) {
                    (None, _) => v,
                    (Some(a), RunOp::Sum) => a + v,
                    (Some(a), RunOp::Prod) => a * v,
                    (Some(a), RunOp::Min) => a.min(v),
                    (Some(a), RunOp::Max) => a.max(v),
                });
            }
            // The running state carries across nulls; the output cell is valid
            // once at least one valid input has been folded in.
            if let Some(a) = acc {
                values[row] = a;
                valid[row] = true;
            }
        }
    }
    Ok(TsArray::F64 { values, valid })
}

/// Running sum within each partition, in order-by order. The column must be
/// numeric (`F64` / `I64`); the result is an `F64` array.
pub fn cumsum(
    frame: &TsDataFrame,
    column: &str,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<TsArray, TsWindowError> {
    cumulative(frame, column, partition_by, order_by, RunOp::Sum)
}

/// Running product within each partition, in order-by order. Numeric only.
pub fn cumprod(
    frame: &TsDataFrame,
    column: &str,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<TsArray, TsWindowError> {
    cumulative(frame, column, partition_by, order_by, RunOp::Prod)
}

/// Running minimum within each partition, in order-by order. Numeric only.
pub fn cummin(
    frame: &TsDataFrame,
    column: &str,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<TsArray, TsWindowError> {
    cumulative(frame, column, partition_by, order_by, RunOp::Min)
}

/// Running maximum within each partition, in order-by order. Numeric only.
pub fn cummax(
    frame: &TsDataFrame,
    column: &str,
    partition_by: &[&str],
    order_by: Option<&str>,
) -> Result<TsArray, TsWindowError> {
    cumulative(frame, column, partition_by, order_by, RunOp::Max)
}

// ---------- over() ----------

/// `agg_expr OVER (PARTITION BY k)`: evaluate the aggregation `agg_expr` over
/// each partition's sub-frame and broadcast the resulting scalar back to every
/// row of that partition (the SQL `agg() OVER (PARTITION BY k)` shape).
/// `agg_expr` must be a top-level [`TsExpr::Agg`]. The result array's type is the
/// reduction's type (`Sum`/`Mean` -> `F64`, `Count` -> `I64`, `Min`/`Max` -> the
/// operand's type), uniform across partitions. A `NaN` reduction (mean of an
/// empty / all-null partition) surfaces as a null cell, never a NaN a downstream
/// consumer must special-case.
pub fn over(
    frame: &TsDataFrame,
    agg_expr: &TsExpr,
    partition_by: &[&str],
) -> Result<TsArray, TsWindowError> {
    if !matches!(agg_expr, TsExpr::Agg(_, _)) {
        return Err(TsWindowError::NotAnAggregation);
    }
    let axis = RowAxis::build(frame);
    let plan = plan(&axis, partition_by, None)?;

    // Reduce each partition to a scalar, recording the partition each row sits
    // in so the scalars scatter back across the row axis.
    let mut scalars: Vec<TsValue> = Vec::with_capacity(plan.partitions.len());
    let mut part_of = vec![0usize; plan.nrows];
    for (pidx, part) in plan.partitions.iter().enumerate() {
        let sub = sub_frame(&axis, part);
        scalars.push(eval_scalar(agg_expr, &sub)?);
        for &row in part {
            part_of[row] = pidx;
        }
    }

    Ok(broadcast(&scalars, &part_of))
}

// Build a sub-frame holding only the partition's rows of every column,
// re-emitted at synthetic monotonic timestamps so the evaluator's
// union-of-timestamps row axis lines up 1:1 with the partition's rows in
// partition order. A missing (null) cell is not pushed, so the reduction sees
// the same validity the partition's slice of the parent axis has.
fn sub_frame(axis: &RowAxis, rows: &[usize]) -> TsDataFrame {
    use subms_ts::{TsColumn, TsSeries};

    let mut f = TsDataFrame::new();
    for (ci, name) in axis.names.iter().enumerate() {
        let cells = &axis.columns[ci];
        let col = match column_kind(cells) {
            Kind::F64 => {
                let mut s = TsSeries::<f64>::with_capacity(rows.len());
                for (synth, &r) in rows.iter().enumerate() {
                    if let Some(TsValue::F64(x)) = cells[r] {
                        let _ = s.push(synth as i64, x);
                    }
                }
                TsColumn::F64(s)
            }
            Kind::I64 => {
                let mut s = TsSeries::<i64>::with_capacity(rows.len());
                for (synth, &r) in rows.iter().enumerate() {
                    if let Some(TsValue::I64(x)) = cells[r] {
                        let _ = s.push(synth as i64, x);
                    }
                }
                TsColumn::I64(s)
            }
            Kind::Bool => {
                let mut s = TsSeries::<bool>::with_capacity(rows.len());
                for (synth, &r) in rows.iter().enumerate() {
                    if let Some(TsValue::Bool(x)) = cells[r] {
                        let _ = s.push(synth as i64, x);
                    }
                }
                TsColumn::Bool(s)
            }
            Kind::Str => {
                let mut s = TsSeries::<String>::with_capacity(rows.len());
                for (synth, &r) in rows.iter().enumerate() {
                    if let Some(TsValue::Str(x)) = &cells[r] {
                        let _ = s.push(synth as i64, x.clone());
                    }
                }
                TsColumn::Str(s)
            }
        };
        // axis names are distinct (frame columns are), so no dup.
        f.push_column(name.clone(), col).unwrap();
    }
    f
}

// Scatter each partition's scalar across its rows. The result type is the
// scalars' uniform type; a Null / NaN scalar broadcasts as an invalid cell.
fn broadcast(scalars: &[TsValue], part_of: &[usize]) -> TsArray {
    let kind = scalars
        .iter()
        .find_map(|v| match v {
            TsValue::F64(x) if x.is_nan() => None,
            TsValue::Null => None,
            TsValue::F64(_) => Some(Kind::F64),
            TsValue::I64(_) => Some(Kind::I64),
            TsValue::Bool(_) => Some(Kind::Bool),
            TsValue::Str(_) => Some(Kind::Str),
            _ => None,
        })
        .unwrap_or(Kind::F64);

    let n = part_of.len();
    match kind {
        Kind::I64 => {
            let mut values = vec![0i64; n];
            let mut valid = vec![false; n];
            for (i, &p) in part_of.iter().enumerate() {
                if let TsValue::I64(x) = scalars[p] {
                    values[i] = x;
                    valid[i] = true;
                }
            }
            TsArray::I64 { values, valid }
        }
        Kind::Bool => {
            let mut values = vec![false; n];
            let mut valid = vec![false; n];
            for (i, &p) in part_of.iter().enumerate() {
                if let TsValue::Bool(x) = scalars[p] {
                    values[i] = x;
                    valid[i] = true;
                }
            }
            TsArray::Bool { values, valid }
        }
        Kind::Str => {
            let mut values = vec![String::new(); n];
            let mut valid = vec![false; n];
            for (i, &p) in part_of.iter().enumerate() {
                if let TsValue::Str(x) = &scalars[p] {
                    values[i] = x.clone();
                    valid[i] = true;
                }
            }
            TsArray::Str { values, valid }
        }
        Kind::F64 => {
            let mut values = vec![0.0f64; n];
            let mut valid = vec![false; n];
            for (i, &p) in part_of.iter().enumerate() {
                let f = match scalars[p] {
                    TsValue::F64(x) => Some(x),
                    TsValue::I64(x) => Some(x as f64),
                    _ => None,
                };
                if let Some(x) = f
                    && !x.is_nan()
                {
                    values[i] = x;
                    valid[i] = true;
                }
            }
            TsArray::F64 { values, valid }
        }
    }
}
