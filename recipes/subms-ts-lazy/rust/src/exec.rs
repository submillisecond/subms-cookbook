//! The executor. [`run_plan`] walks an optimised [`PlanNode`] list and produces
//! a [`ResultFrame`] - a ts axis plus one named [`TsArray`] per output column.
//!
//! Execution materialises the source frame's union-of-timestamps row axis once
//! (via `subms-ts-expr` eval), then each node is a slice / gather / append over
//! that materialised state:
//!
//! - `Filter` evals the predicate to a Bool [`TsArray`] and gathers passing rows.
//! - `WithColumn` evals the expr to a [`TsArray`] and appends (or replaces) it.
//! - `SortBy` computes a stable permutation over a key column (nulls last).
//! - `Limit` truncates the row count.
//! - `Agg` (terminal) reduces each expr via `eval_scalar` to a one-row frame.
//!
//! A [`ResultFrame`] keeps rows in pipeline order, so a post-sort axis need not
//! be monotonic. [`ResultFrame::into_data_frame`] is the round-trip back to a
//! [`TsDataFrame`]: it orders rows by ts (the frame's no-out-of-order invariant)
//! and gathers each column's present cells into a typed series.

use subms_ts::{TsColumn, TsDataFrame, TsDataType, TsSeries, TsValue};
use subms_ts_expr::{TsArray, TsExpr, TsExprError, eval, eval_scalar};

use crate::plan::PlanNode;

/// Errors `collect` / `certify`-time execution can raise. Wraps the evaluator's
/// errors plus the lazy-specific "sorted on a column that isn't there".
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LazyError {
    /// A node's expression failed to evaluate.
    Eval(TsExprError),
    /// `sort_by` named a column not present at that point in the pipeline.
    UnknownSortColumn(String),
    /// A `Filter` predicate did not evaluate to a Bool array.
    NonBoolPredicate(TsDataType),
}

impl std::fmt::Display for LazyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LazyError::Eval(e) => write!(f, "eval error: {e}"),
            LazyError::UnknownSortColumn(c) => write!(f, "sort_by: unknown column {c}"),
            LazyError::NonBoolPredicate(t) => {
                write!(f, "filter predicate must be Bool, got {t:?}")
            }
        }
    }
}

impl std::error::Error for LazyError {}

impl From<TsExprError> for LazyError {
    fn from(e: TsExprError) -> Self {
        LazyError::Eval(e)
    }
}

/// The executed result: a ts axis plus named typed columns, kept in pipeline
/// (post-filter / post-sort) row order. Distinct from a `TsDataFrame` because
/// a sorted result's ts axis is not necessarily monotonic.
#[derive(Clone, Debug, PartialEq)]
pub struct ResultFrame {
    ts: Vec<i64>,
    names: Vec<String>,
    columns: Vec<TsArray>,
}

impl ResultFrame {
    pub fn nrows(&self) -> usize {
        self.ts.len()
    }

    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.columns.is_empty() || self.ts.is_empty()
    }

    pub fn ts(&self) -> &[i64] {
        &self.ts
    }

    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.names.iter().map(|s| s.as_str())
    }

    /// The typed array for a named column, or `None` if absent.
    pub fn column(&self, name: &str) -> Option<&TsArray> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| &self.columns[i])
    }

    /// The boxed cell at `(row, column)`, `None` if the cell is null or the
    /// column is absent.
    pub fn cell(&self, row: usize, name: &str) -> Option<TsValue> {
        self.column(name).and_then(|c| c.get(row))
    }

    /// Round-trip back to a [`TsDataFrame`]: order rows by ts (the frame's
    /// no-out-of-order invariant) and gather each column's present cells into a
    /// typed series. A null cell becomes a gap (the series simply has no point
    /// at that ts), which is the frame's null model.
    pub fn into_data_frame(self) -> TsDataFrame {
        let n = self.ts.len();
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by_key(|&i| self.ts[i]);

        let mut out = TsDataFrame::new();
        for (name, arr) in self.names.iter().zip(self.columns.iter()) {
            let col = gather_column(arr, &self.ts, &order);
            out.push_column(name.clone(), col)
                .expect("ResultFrame column names are unique by construction");
        }
        out
    }
}

fn gather_column(arr: &TsArray, ts: &[i64], order: &[usize]) -> TsColumn {
    match arr {
        TsArray::F64 { values, valid } => {
            let mut s = TsSeries::<f64>::new();
            for &i in order {
                if valid[i] {
                    let _ = s.push(ts[i], values[i]);
                }
            }
            TsColumn::F64(s)
        }
        TsArray::I64 { values, valid } => {
            let mut s = TsSeries::<i64>::new();
            for &i in order {
                if valid[i] {
                    let _ = s.push(ts[i], values[i]);
                }
            }
            TsColumn::I64(s)
        }
        TsArray::Bool { values, valid } => {
            let mut s = TsSeries::<bool>::new();
            for &i in order {
                if valid[i] {
                    let _ = s.push(ts[i], values[i]);
                }
            }
            TsColumn::Bool(s)
        }
        TsArray::Str { values, valid } => {
            let mut s = TsSeries::<String>::new();
            for &i in order {
                if valid[i] {
                    let _ = s.push(ts[i], values[i].clone());
                }
            }
            TsColumn::Str(s)
        }
    }
}

// The in-flight state: a ts axis plus parallel named typed columns over the row
// axis. Every transform rewrites this in place; the result is wrapped into a
// ResultFrame at the end.
struct State {
    ts: Vec<i64>,
    names: Vec<String>,
    columns: Vec<TsArray>,
}

impl State {
    fn materialise(frame: &TsDataFrame) -> Result<State, LazyError> {
        let names: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
        let ts: Vec<i64> = frame.aligned().map(|(t, _)| t).collect();
        let mut columns = Vec::with_capacity(names.len());
        for name in &names {
            // Evaluating Col(name) projects the column onto the row axis,
            // reusing the evaluator's typed null-aware projection.
            columns.push(eval(&TsExpr::col(name), frame)?);
        }
        Ok(State { ts, names, columns })
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    fn nrows(&self) -> usize {
        self.ts.len()
    }

    // A frame the evaluator can run against that preserves the in-flight ROW
    // ORDER exactly. Expr eval is positional/elementwise over the row axis, so
    // its output array maps back 1:1 to the in-flight rows by position. The
    // frame uses the row index as a synthetic monotonic ts (a real ts axis may
    // be non-monotonic after a sort, which TsSeries::push would reject, and the
    // eval does not depend on the ts values - only on per-row alignment).
    fn frame_for_eval(&self) -> TsDataFrame {
        let n = self.nrows();
        let mut out = TsDataFrame::new();
        // An always-valid anchor column pins every row index into the aligned
        // view's union axis, so a row whose every real column is null at that
        // position does not silently collapse and break positional alignment.
        // It is never referenced by a user expr and never surfaces in a result.
        let mut anchor = TsSeries::<i64>::new();
        for i in 0..n as i64 {
            let _ = anchor.push(i, i);
        }
        out.push_column(ANCHOR, TsColumn::I64(anchor))
            .expect("anchor column name is reserved");
        for (name, arr) in self.names.iter().zip(self.columns.iter()) {
            let col = gather_column_positional(arr, n);
            out.push_column(name.clone(), col)
                .expect("in-flight column names are unique by construction");
        }
        out
    }
}

// Reserved name for the eval anchor column. Underscore-prefixed and unlikely to
// collide with a real column; a user Select/Filter never names it.
const ANCHOR: &str = "__subms_lazy_row";

// Rebuild a column from a TsArray using the row index as ts, so the frame's row
// axis is exactly the array's positions. A null cell becomes a gap at that ts.
fn gather_column_positional(arr: &TsArray, n: usize) -> TsColumn {
    let order: Vec<usize> = (0..n).collect();
    let ts: Vec<i64> = (0..n as i64).collect();
    gather_column(arr, &ts, &order)
}

/// Execute an already-optimised node list against `source`, returning a
/// [`ResultFrame`].
pub fn run_plan(source: &TsDataFrame, nodes: &[PlanNode]) -> Result<ResultFrame, LazyError> {
    let mut state = State::materialise(source)?;

    for node in nodes {
        match node {
            PlanNode::Select(cols) => apply_select(&mut state, cols),
            PlanNode::Filter(pred) => apply_filter(&mut state, pred)?,
            PlanNode::WithColumn(name, expr) => apply_with_column(&mut state, name, expr)?,
            PlanNode::SortBy { column, ascending } => apply_sort(&mut state, column, *ascending)?,
            PlanNode::Limit(n) => apply_limit(&mut state, *n),
            PlanNode::Agg(aggs) => return apply_agg(&state, aggs),
        }
    }

    Ok(ResultFrame {
        ts: state.ts,
        names: state.names,
        columns: state.columns,
    })
}

fn apply_select(state: &mut State, cols: &[String]) {
    let mut names = Vec::with_capacity(cols.len());
    let mut columns = Vec::with_capacity(cols.len());
    for c in cols {
        if let Some(i) = state.index_of(c) {
            names.push(state.names[i].clone());
            columns.push(state.columns[i].clone());
        }
    }
    state.names = names;
    state.columns = columns;
}

fn apply_filter(state: &mut State, pred: &TsExpr) -> Result<(), LazyError> {
    let frame = state.frame_for_eval();
    let mask = eval(pred, &frame)?;
    let (values, valid) = match &mask {
        TsArray::Bool { values, valid } => (values, valid),
        other => return Err(LazyError::NonBoolPredicate(other.data_type())),
    };
    // The mask aligns 1:1 with the in-flight rows by position (the anchor column
    // pins every row index into the eval frame's union axis). Keep a row only
    // where its mask cell is a present, true Bool - a null predicate is not a
    // pass.
    let keep: Vec<bool> = (0..state.nrows())
        .map(|i| valid.get(i).copied().unwrap_or(false) && values.get(i).copied().unwrap_or(false))
        .collect();
    gather_rows(state, &keep);
    Ok(())
}

fn gather_rows(state: &mut State, keep: &[bool]) {
    let idx: Vec<usize> = (0..state.nrows()).filter(|&i| keep[i]).collect();
    state.ts = idx.iter().map(|&i| state.ts[i]).collect();
    for col in state.columns.iter_mut() {
        *col = take_rows(col, &idx);
    }
}

fn apply_with_column(state: &mut State, name: &str, expr: &TsExpr) -> Result<(), LazyError> {
    let frame = state.frame_for_eval();
    let arr = eval(expr, &frame)?;
    if let Some(i) = state.index_of(name) {
        state.columns[i] = arr;
    } else {
        state.names.push(name.to_string());
        state.columns.push(arr);
    }
    Ok(())
}

fn apply_sort(state: &mut State, column: &str, ascending: bool) -> Result<(), LazyError> {
    let i = state
        .index_of(column)
        .ok_or_else(|| LazyError::UnknownSortColumn(column.to_string()))?;
    let key = &state.columns[i];
    let n = state.nrows();
    let mut order: Vec<usize> = (0..n).collect();
    // Stable sort. Null keys sort last in both directions.
    order.sort_by(|&a, &b| cmp_cells(key, a, b, ascending));
    apply_permutation(state, &order);
    Ok(())
}

fn apply_permutation(state: &mut State, order: &[usize]) {
    state.ts = order.iter().map(|&i| state.ts[i]).collect();
    for col in state.columns.iter_mut() {
        *col = take_rows(col, order);
    }
}

fn apply_limit(state: &mut State, n: usize) {
    let keep = n.min(state.nrows());
    state.ts.truncate(keep);
    for col in state.columns.iter_mut() {
        *col = truncate_array(col, keep);
    }
}

fn apply_agg(state: &State, aggs: &[(String, TsExpr)]) -> Result<ResultFrame, LazyError> {
    let frame = state.frame_for_eval();
    let mut names = Vec::with_capacity(aggs.len());
    let mut columns = Vec::with_capacity(aggs.len());
    for (name, expr) in aggs {
        let scalar = eval_scalar(expr, &frame)?;
        names.push(name.clone());
        columns.push(scalar_to_array(&scalar));
    }
    // A whole-frame aggregate is a single output row; ts 0 is a placeholder
    // (the reduction collapses the time axis).
    Ok(ResultFrame {
        ts: vec![0],
        names,
        columns,
    })
}

fn cmp_cells(arr: &TsArray, a: usize, b: usize, ascending: bool) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    let (va, vb) = (cell_valid(arr, a), cell_valid(arr, b));
    match (va, vb) {
        (false, false) => Ordering::Equal,
        (false, true) => Ordering::Greater, // nulls last
        (true, false) => Ordering::Less,
        (true, true) => {
            let ord = cmp_present(arr, a, b);
            if ascending { ord } else { ord.reverse() }
        }
    }
}

fn cell_valid(arr: &TsArray, i: usize) -> bool {
    arr.valid().get(i).copied().unwrap_or(false)
}

fn cmp_present(arr: &TsArray, a: usize, b: usize) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match arr {
        TsArray::F64 { values, .. } => values[a].partial_cmp(&values[b]).unwrap_or(Ordering::Equal),
        TsArray::I64 { values, .. } => values[a].cmp(&values[b]),
        TsArray::Bool { values, .. } => values[a].cmp(&values[b]),
        TsArray::Str { values, .. } => values[a].cmp(&values[b]),
    }
}

fn take_rows(arr: &TsArray, idx: &[usize]) -> TsArray {
    match arr {
        TsArray::F64 { values, valid } => TsArray::F64 {
            values: idx.iter().map(|&i| values[i]).collect(),
            valid: idx.iter().map(|&i| valid[i]).collect(),
        },
        TsArray::I64 { values, valid } => TsArray::I64 {
            values: idx.iter().map(|&i| values[i]).collect(),
            valid: idx.iter().map(|&i| valid[i]).collect(),
        },
        TsArray::Bool { values, valid } => TsArray::Bool {
            values: idx.iter().map(|&i| values[i]).collect(),
            valid: idx.iter().map(|&i| valid[i]).collect(),
        },
        TsArray::Str { values, valid } => TsArray::Str {
            values: idx.iter().map(|&i| values[i].clone()).collect(),
            valid: idx.iter().map(|&i| valid[i]).collect(),
        },
    }
}

fn truncate_array(arr: &TsArray, n: usize) -> TsArray {
    let idx: Vec<usize> = (0..n).collect();
    take_rows(arr, &idx)
}

fn scalar_to_array(v: &TsValue) -> TsArray {
    match v {
        TsValue::F64(x) => TsArray::F64 {
            values: vec![*x],
            valid: vec![true],
        },
        TsValue::I64(x) => TsArray::I64 {
            values: vec![*x],
            valid: vec![true],
        },
        TsValue::Bool(x) => TsArray::Bool {
            values: vec![*x],
            valid: vec![true],
        },
        TsValue::Str(s) => TsArray::Str {
            values: vec![s.clone()],
            valid: vec![true],
        },
        // A null reduction (Min/Max over all-null) is a single null F64 cell.
        _ => TsArray::F64 {
            values: vec![0.0],
            valid: vec![false],
        },
    }
}
