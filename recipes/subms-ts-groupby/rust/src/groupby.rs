//! The group-by engine. A [`TsGroupBy`] is a single-pass hash partition of a
//! [`TsDataFrame`]'s aligned rows by the TUPLE of TYPED cell values in the key
//! columns; the follow-on [`TsGroupBy::agg`] reduces a set of [`TsExpr`]
//! aggregations per group through the `subms-ts-expr` evaluator.
//!
//! ## Row model
//!
//! A frame is a bag of named, per-column-typed series. We materialise its
//! union-of-timestamps row axis exactly as the evaluator does (via
//! `frame.aligned()`), so a row is a tuple of `Option<TsValue>` cells, one per
//! column. Group-by keys reference columns by name; the key of a row is the
//! tuple of those columns' cells AT that row - and the key is TYPED, so a `Str`
//! symbol column keys directly on the string, an `I64` column on the integer.
//!
//! ## Null-key policy
//!
//! A row whose key tuple contains ANY null/missing cell is DROPPED - it does
//! not form a group and contributes to no aggregate. This matches the
//! analytical-front default that avoids a `None` bucket leaking into a typed
//! key column. The choice is pinned by a test; callers who want a null bucket
//! coalesce the key upstream before grouping.
//!
//! ## Determinism
//!
//! Output rows are sorted by the key tuple (lexicographic over the typed keys),
//! so the result is reproducible regardless of input row order or hash
//! iteration order. The cross-type order is defined by [`KeyCell`]'s `Ord`: a
//! numeric (`I64`/`F64`) order, then `Bool`, then `Str`; a stored f64 key is
//! always finite (a series rejects non-finite on ingest), so the f64 order is
//! total within that arm.

use std::collections::HashMap;

use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::{TsArray, TsExpr, eval_scalar};

/// Errors the group-by surface can raise.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupByError {
    /// A key column / aggregation referenced a column the frame does not hold.
    UnknownColumn(String),
    /// `group_by` was called with an empty key list - there is nothing to
    /// partition on. Callers wanting a single whole-frame aggregate use the
    /// evaluator's `eval_scalar` directly.
    NoKeys,
    /// An aggregation expression was not a top-level `Agg` (it would be per-row,
    /// not a single reduced value per group).
    NotAnAggregation(String),
}

impl std::fmt::Display for GroupByError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GroupByError::UnknownColumn(name) => write!(f, "unknown column: {name}"),
            GroupByError::NoKeys => write!(f, "group_by requires at least one key column"),
            GroupByError::NotAnAggregation(name) => {
                write!(f, "aggregation '{name}' is not a top-level Agg expression")
            }
        }
    }
}

impl std::error::Error for GroupByError {}

/// A typed, hashable, totally-ordered projection of a [`TsValue`] cell, used as
/// a group key element. The variants carry the underlying value so a key can be
/// rebuilt back into a `TsValue` for the result table. An `F64` keys on its bit
/// pattern (with `-0.0` normalised to `0.0`) so equal values land in one group;
/// non-finite never reaches here (a series rejects it on ingest). Bytes / Map /
/// Array / Null cells are not legal keys and are treated as a null key (the row
/// is dropped) during partitioning.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum KeyCell {
    I64(i64),
    F64(u64),
    Bool(bool),
    Str(String),
}

impl KeyCell {
    fn from_value(v: &TsValue) -> Option<KeyCell> {
        match v {
            TsValue::I64(x) => Some(KeyCell::I64(*x)),
            TsValue::F64(x) => Some(KeyCell::F64(f64_key_bits(*x))),
            TsValue::Bool(b) => Some(KeyCell::Bool(*b)),
            TsValue::Str(s) => Some(KeyCell::Str(s.clone())),
            _ => None,
        }
    }

    fn to_value(&self) -> TsValue {
        match self {
            KeyCell::I64(x) => TsValue::I64(*x),
            KeyCell::F64(bits) => TsValue::F64(f64::from_bits(*bits)),
            KeyCell::Bool(b) => TsValue::Bool(*b),
            KeyCell::Str(s) => TsValue::Str(s.clone()),
        }
    }
}

// Normalise -0.0 to +0.0 so the two zeros share a bit pattern (and a group),
// matching value equality. A stored f64 is always finite, so the resulting bit
// pattern is a faithful ordered key within the F64 arm.
fn f64_key_bits(v: f64) -> u64 {
    let v = if v == 0.0 { 0.0 } else { v };
    v.to_bits()
}

/// The materialised row axis of a frame: column names + dtypes + per-column
/// dense `Option<TsValue>` cells over the union-of-timestamps rows. Built once
/// so the partition pass and every per-group sub-frame share it.
#[derive(Debug)]
struct RowAxis {
    names: Vec<String>,
    columns: Vec<Vec<Option<TsValue>>>,
    timestamps: Vec<i64>,
}

impl RowAxis {
    fn build(frame: &TsDataFrame) -> RowAxis {
        let names: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
        let mut columns: Vec<Vec<Option<TsValue>>> = vec![Vec::new(); names.len()];
        let mut timestamps = Vec::new();
        for (ts, row) in frame.aligned() {
            for (i, cell) in row.into_iter().enumerate() {
                columns[i].push(cell);
            }
            timestamps.push(ts);
        }
        RowAxis {
            names,
            columns,
            timestamps,
        }
    }

    fn nrows(&self) -> usize {
        self.timestamps.len()
    }

    fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }
}

/// A built partition of a frame's rows by a tuple of key columns, ready to be
/// aggregated. Construct with [`group_by`].
#[derive(Debug)]
pub struct TsGroupBy {
    keys: Vec<String>,
    // Per group: the typed key tuple + the row indices (into the row axis) that
    // fell into it. Groups are in deterministic key-sorted order.
    groups: Vec<GroupSlot>,
    axis: RowAxis,
}

#[derive(Debug)]
struct GroupSlot {
    key: Vec<KeyCell>,
    rows: Vec<usize>,
}

/// Partition `frame`'s rows by the tuple of TYPED values in `keys`. A key column
/// may be `Str` (symbol), `I64` (id / date-as-nanos), `F64`, or `Bool`; the key
/// is the tuple of those columns' typed cells. Rows with a null cell in any key
/// column (or a non-hashable cell - bytes / map / array / null) are dropped (see
/// the module docs). Returns an error for an empty key list or an unknown
/// column.
pub fn group_by(frame: &TsDataFrame, keys: &[&str]) -> Result<TsGroupBy, GroupByError> {
    if keys.is_empty() {
        return Err(GroupByError::NoKeys);
    }

    let axis = RowAxis::build(frame);
    let key_idx: Vec<usize> = keys
        .iter()
        .map(|k| {
            axis.index_of(k)
                .ok_or_else(|| GroupByError::UnknownColumn((*k).to_string()))
        })
        .collect::<Result<_, _>>()?;

    // Single-pass hash partition. The map keys on the typed key tuple; the value
    // is the index into `groups`, which accumulates the first-seen key + rows.
    let mut index: HashMap<Vec<KeyCell>, usize> = HashMap::new();
    let mut groups: Vec<GroupSlot> = Vec::new();
    for r in 0..axis.nrows() {
        let mut key = Vec::with_capacity(key_idx.len());
        let mut droppable = false;
        for &ci in &key_idx {
            match axis.columns[ci][r].as_ref().and_then(KeyCell::from_value) {
                Some(cell) => key.push(cell),
                None => {
                    droppable = true;
                    break;
                }
            }
        }
        if droppable {
            continue;
        }
        let slot = *index.entry(key.clone()).or_insert_with(|| {
            groups.push(GroupSlot {
                key,
                rows: Vec::new(),
            });
            groups.len() - 1
        });
        groups[slot].rows.push(r);
    }

    groups.sort_by(|a, b| a.key.cmp(&b.key));

    Ok(TsGroupBy {
        keys: keys.iter().map(|k| k.to_string()).collect(),
        groups,
        axis,
    })
}

impl TsGroupBy {
    /// Number of groups (one per distinct key tuple).
    pub fn ngroups(&self) -> usize {
        self.groups.len()
    }

    /// The typed key tuple of group `g`, in key-column order.
    pub fn key(&self, g: usize) -> Vec<TsValue> {
        self.groups[g].key.iter().map(KeyCell::to_value).collect()
    }

    /// Build a frame holding only the rows of group `g`. Each column's values are
    /// re-emitted at synthetic monotonic timestamps so the evaluator's
    /// union-of-timestamps row axis lines up 1:1 with the group's rows. Missing
    /// (null) cells are not pushed, which the evaluator reads back as a derived
    /// null at that row - identical to the parent frame's behaviour.
    fn group_frame(&self, g: usize) -> TsDataFrame {
        let rows = &self.groups[g].rows;
        let mut f = TsDataFrame::new();
        for (ci, name) in self.axis.names.iter().enumerate() {
            let col = build_group_column(&self.axis.columns[ci], rows);
            // unwrap: axis names are distinct (frame columns are), so no dup.
            f.push_column(name.clone(), col).unwrap();
        }
        f
    }

    /// Reduce each `(out_name, expr)` aggregation per group, returning a table
    /// with one row per group: the key columns followed by the aggregated
    /// columns. Each `expr` must be a top-level `Agg` (e.g.
    /// `TsExpr::col("x").sum()`); a non-agg expr is rejected because it would be
    /// per-row, not a single value per group.
    pub fn agg(&self, aggs: &[(&str, TsExpr)]) -> Result<TsGroupResult, GroupByError> {
        for (name, expr) in aggs {
            if !matches!(expr, TsExpr::Agg(_, _)) {
                return Err(GroupByError::NotAnAggregation((*name).to_string()));
            }
        }

        let ngroups = self.groups.len();

        // Key columns: each rebuilt as a typed, fully-valid TsArray from the
        // group keys. The key cells share a type within a column (one key column
        // is one dtype), so we read the first group's cell to pick the builder.
        let mut builder = ResultBuilder::new();
        for (ki, key_name) in self.keys.iter().enumerate() {
            let cells: Vec<KeyCell> = self.groups.iter().map(|s| s.key[ki].clone()).collect();
            builder.push(key_name.clone(), key_array(&cells));
        }

        // Aggregation columns: per group, build the sub-frame and eval each expr,
        // then assemble one typed TsArray per aggregation across groups.
        let mut scalars: Vec<Vec<TsValue>> = vec![Vec::with_capacity(ngroups); aggs.len()];
        for g in 0..ngroups {
            let frame = self.group_frame(g);
            for (ai, (_name, expr)) in aggs.iter().enumerate() {
                let scalar = eval_scalar(expr, &frame)
                    .map_err(|_| GroupByError::UnknownColumn(describe_expr_col(expr)))?;
                scalars[ai].push(scalar);
            }
        }
        for ((name, _expr), col) in aggs.iter().zip(scalars) {
            builder.push((*name).to_string(), scalar_array(&col));
        }

        Ok(builder.finish(ngroups))
    }
}

// Build a sub-frame column for one group: re-emit the kept rows' values at
// synthetic monotonic timestamps, preserving the parent column's type.
fn build_group_column(cells: &[Option<TsValue>], rows: &[usize]) -> TsColumn {
    // Pick the column type from the first present cell across the whole column;
    // a column whose every cell in this group is null still needs the right
    // empty-typed series so an Agg over it reduces under the correct type.
    let dtype = cells
        .iter()
        .find_map(|c| c.as_ref().map(value_kind))
        .unwrap_or(ValueKind::F64);

    macro_rules! collect_typed {
        ($variant:path, $extract:expr, $tser:ty) => {{
            let mut s = <$tser>::with_capacity(rows.len());
            for (synth_ts, &r) in rows.iter().enumerate() {
                if let Some(v) = cells[r].as_ref().and_then($extract) {
                    // synthetic ts is strictly increasing and the value came out
                    // of a series that already rejected non-finite, so push
                    // cannot fail.
                    let _ = s.push(synth_ts as i64, v);
                }
            }
            $variant(s)
        }};
    }

    match dtype {
        ValueKind::F64 => collect_typed!(
            TsColumn::F64,
            |v: &TsValue| match v {
                TsValue::F64(x) => Some(*x),
                _ => None,
            },
            TsSeries<f64>
        ),
        ValueKind::I64 => collect_typed!(
            TsColumn::I64,
            |v: &TsValue| match v {
                TsValue::I64(x) => Some(*x),
                _ => None,
            },
            TsSeries<i64>
        ),
        ValueKind::Bool => collect_typed!(
            TsColumn::Bool,
            |v: &TsValue| match v {
                TsValue::Bool(x) => Some(*x),
                _ => None,
            },
            TsSeries<bool>
        ),
        ValueKind::Str => collect_typed!(
            TsColumn::Str,
            |v: &TsValue| match v {
                TsValue::Str(x) => Some(x.clone()),
                _ => None,
            },
            TsSeries<String>
        ),
    }
}

#[derive(Copy, Clone)]
enum ValueKind {
    F64,
    I64,
    Bool,
    Str,
}

fn value_kind(v: &TsValue) -> ValueKind {
    match v {
        TsValue::F64(_) => ValueKind::F64,
        TsValue::I64(_) => ValueKind::I64,
        TsValue::Bool(_) => ValueKind::Bool,
        TsValue::Str(_) => ValueKind::Str,
        // bytes / map / array / null never reach a typed group column.
        _ => ValueKind::F64,
    }
}

// Build a fully-valid TsArray from typed group keys. Keys in one column share a
// type, so the first cell picks the variant; an empty group set defaults to F64.
fn key_array(cells: &[KeyCell]) -> TsArray {
    match cells.first() {
        Some(KeyCell::I64(_)) => TsArray::I64 {
            values: cells
                .iter()
                .map(|c| match c {
                    KeyCell::I64(x) => *x,
                    _ => 0,
                })
                .collect(),
            valid: vec![true; cells.len()],
        },
        Some(KeyCell::F64(_)) => TsArray::F64 {
            values: cells
                .iter()
                .map(|c| match c {
                    KeyCell::F64(b) => f64::from_bits(*b),
                    _ => 0.0,
                })
                .collect(),
            valid: vec![true; cells.len()],
        },
        Some(KeyCell::Bool(_)) => TsArray::Bool {
            values: cells
                .iter()
                .map(|c| matches!(c, KeyCell::Bool(true)))
                .collect(),
            valid: vec![true; cells.len()],
        },
        Some(KeyCell::Str(_)) => TsArray::Str {
            values: cells
                .iter()
                .map(|c| match c {
                    KeyCell::Str(s) => s.clone(),
                    _ => String::new(),
                })
                .collect(),
            valid: vec![true; cells.len()],
        },
        None => TsArray::F64 {
            values: Vec::new(),
            valid: Vec::new(),
        },
    }
}

// Build a TsArray from per-group scalar reduction results. The aggregation type
// is uniform across groups (the same expr over same-typed columns), so the
// first non-null scalar picks the variant; a Null cell becomes an invalid slot.
// Mean over an empty / all-null group is NaN, which we surface as a null cell.
fn scalar_array(scalars: &[TsValue]) -> TsArray {
    let kind = scalars
        .iter()
        .find_map(|v| match v {
            TsValue::F64(x) if x.is_nan() => None,
            TsValue::Null => None,
            other => Some(value_kind(other)),
        })
        .unwrap_or(ValueKind::F64);

    let n = scalars.len();
    match kind {
        ValueKind::I64 => {
            let mut values = vec![0i64; n];
            let mut valid = vec![false; n];
            for (i, v) in scalars.iter().enumerate() {
                if let TsValue::I64(x) = v {
                    values[i] = *x;
                    valid[i] = true;
                }
            }
            TsArray::I64 { values, valid }
        }
        ValueKind::Bool => {
            let mut values = vec![false; n];
            let mut valid = vec![false; n];
            for (i, v) in scalars.iter().enumerate() {
                if let TsValue::Bool(x) = v {
                    values[i] = *x;
                    valid[i] = true;
                }
            }
            TsArray::Bool { values, valid }
        }
        ValueKind::Str => {
            let mut values = vec![String::new(); n];
            let mut valid = vec![false; n];
            for (i, v) in scalars.iter().enumerate() {
                if let TsValue::Str(x) = v {
                    values[i] = x.clone();
                    valid[i] = true;
                }
            }
            TsArray::Str { values, valid }
        }
        ValueKind::F64 => {
            let mut values = vec![0.0; n];
            let mut valid = vec![false; n];
            for (i, v) in scalars.iter().enumerate() {
                let f = match v {
                    TsValue::F64(x) => Some(*x),
                    TsValue::I64(x) => Some(*x as f64),
                    _ => None,
                };
                // NaN (empty / all-null Mean) surfaces as a null, never a NaN a
                // downstream consumer must special-case.
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

// Best-effort column name for an UnknownColumn error raised inside eval_scalar.
fn describe_expr_col(expr: &TsExpr) -> String {
    fn first_col(e: &TsExpr) -> Option<String> {
        match e {
            TsExpr::Col(n) => Some(n.clone()),
            TsExpr::Lit(_) => None,
            TsExpr::Unary(_, c) | TsExpr::Agg(_, c) => first_col(c),
            TsExpr::Binary(_, a, b) | TsExpr::Compare(_, a, b) => {
                first_col(a).or_else(|| first_col(b))
            }
            TsExpr::When {
                cond,
                then,
                otherwise,
            } => first_col(cond)
                .or_else(|| first_col(then))
                .or_else(|| first_col(otherwise)),
        }
    }
    first_col(expr).unwrap_or_else(|| "<unknown>".to_string())
}

// Internal accumulator for the result's named columns.
struct ResultBuilder {
    columns: Vec<(String, TsArray)>,
}

impl ResultBuilder {
    fn new() -> Self {
        Self {
            columns: Vec::new(),
        }
    }

    fn push(&mut self, name: String, arr: TsArray) {
        self.columns.push((name, arr));
    }

    fn finish(self, nrows: usize) -> TsGroupResult {
        TsGroupResult {
            columns: self.columns,
            nrows,
        }
    }
}

/// The result of an analytical operator: a positional named-`TsArray` table, one
/// row per group, key columns first then aggregated columns. Rows are in
/// deterministic (key-sorted) order. This is the shared result shape across the
/// group-by / value_counts / unique surface.
#[derive(Clone, Debug, PartialEq)]
pub struct TsGroupResult {
    columns: Vec<(String, TsArray)>,
    nrows: usize,
}

impl TsGroupResult {
    /// Number of result rows (one per group).
    pub fn nrows(&self) -> usize {
        self.nrows
    }

    /// Number of columns (key columns + aggregated columns).
    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    /// The column names, in order (key columns then aggregated columns).
    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|(n, _)| n.as_str())
    }

    /// The typed array of column `name`, or `None` if absent.
    pub fn column(&self, name: &str) -> Option<&TsArray> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, a)| a)
    }

    /// The boxed value of column `name` at `row`, or `None` if the column is
    /// absent or that cell is null.
    pub fn value(&self, name: &str, row: usize) -> Option<TsValue> {
        self.column(name).and_then(|a| a.get(row))
    }
}

/// Group `column` by value and count occurrences, returning a [`TsGroupResult`]
/// with the key column plus a `count` (`I64`) column, sorted by DESCENDING count
/// (ties broken by ascending key). The analytical `Series.value_counts`. Null /
/// non-hashable cells are not counted.
pub fn value_counts(frame: &TsDataFrame, column: &str) -> Result<TsGroupResult, GroupByError> {
    let gb = group_by(frame, &[column])?;
    let mut order: Vec<usize> = (0..gb.groups.len()).collect();
    order.sort_by(|&a, &b| {
        gb.groups[b]
            .rows
            .len()
            .cmp(&gb.groups[a].rows.len())
            .then_with(|| gb.groups[a].key.cmp(&gb.groups[b].key))
    });

    let key_cells: Vec<KeyCell> = order.iter().map(|&g| gb.groups[g].key[0].clone()).collect();
    let counts: Vec<i64> = order
        .iter()
        .map(|&g| gb.groups[g].rows.len() as i64)
        .collect();

    let mut builder = ResultBuilder::new();
    builder.push(column.to_string(), key_array(&key_cells));
    let n = counts.len();
    builder.push(
        "count".to_string(),
        TsArray::I64 {
            values: counts,
            valid: vec![true; n],
        },
    );
    Ok(builder.finish(n))
}

/// The distinct key tuples present in `frame` over `columns`, as a
/// [`TsGroupResult`] of just the key columns, in deterministic (key-sorted)
/// order. Rows with a null in any of `columns` are dropped, same as
/// `group_by`. The analytical `DataFrame.unique(subset=columns)`.
pub fn unique(frame: &TsDataFrame, columns: &[&str]) -> Result<TsGroupResult, GroupByError> {
    let gb = group_by(frame, columns)?;
    let ngroups = gb.groups.len();
    let mut builder = ResultBuilder::new();
    for (ki, name) in gb.keys.iter().enumerate() {
        let cells: Vec<KeyCell> = gb.groups.iter().map(|s| s.key[ki].clone()).collect();
        builder.push(name.clone(), key_array(&cells));
    }
    Ok(builder.finish(ngroups))
}

/// Reorder `frame`'s rows by `column`'s value, largest first, and return the top
/// `k` as a new [`TsDataFrame`] (every column projected, at fresh synthetic
/// monotonic timestamps). `column` may be `F64` or `I64`. Rows with a null in
/// `column` are excluded from ranking. Ties broken by ascending original row
/// index for determinism. The analytical `DataFrame.top_k(k, by=column)`.
pub fn top_k(frame: &TsDataFrame, column: &str, k: usize) -> Result<TsDataFrame, GroupByError> {
    let axis = RowAxis::build(frame);
    let ci = axis
        .index_of(column)
        .ok_or_else(|| GroupByError::UnknownColumn(column.to_string()))?;

    let mut ranked: Vec<usize> = (0..axis.nrows())
        .filter(|&r| axis.columns[ci][r].as_ref().and_then(numeric_of).is_some())
        .collect();
    ranked.sort_by(|&a, &b| {
        let va = numeric_of(axis.columns[ci][a].as_ref().unwrap()).unwrap();
        let vb = numeric_of(axis.columns[ci][b].as_ref().unwrap()).unwrap();
        vb.partial_cmp(&va)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(&b))
    });
    ranked.truncate(k);

    Ok(reorder_frame(&axis, &ranked))
}

/// Sort `frame`'s rows by `columns` (lexicographic, multi-key) and return a new
/// [`TsDataFrame`] in the chosen order (every column projected, at fresh
/// synthetic monotonic timestamps). `ascending` flips the direction for ALL
/// keys. Rows with a null in a sort key sort last (nulls-last). The analytical
/// `DataFrame.sort(by=columns)`.
pub fn sort_by(
    frame: &TsDataFrame,
    columns: &[&str],
    ascending: bool,
) -> Result<TsDataFrame, GroupByError> {
    if columns.is_empty() {
        return Err(GroupByError::NoKeys);
    }
    let axis = RowAxis::build(frame);
    let key_idx: Vec<usize> = columns
        .iter()
        .map(|c| {
            axis.index_of(c)
                .ok_or_else(|| GroupByError::UnknownColumn((*c).to_string()))
        })
        .collect::<Result<_, _>>()?;

    let mut order: Vec<usize> = (0..axis.nrows()).collect();
    order.sort_by(|&a, &b| {
        for &ki in &key_idx {
            let va = axis.columns[ki][a].as_ref().and_then(KeyCell::from_value);
            let vb = axis.columns[ki][b].as_ref().and_then(KeyCell::from_value);
            let cmp = match (va, vb) {
                (Some(x), Some(y)) => {
                    let base = x.cmp(&y);
                    if ascending { base } else { base.reverse() }
                }
                // nulls-last regardless of direction.
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            };
            if cmp != std::cmp::Ordering::Equal {
                return cmp;
            }
        }
        a.cmp(&b)
    });

    Ok(reorder_frame(&axis, &order))
}

// A numeric view of a cell (F64 / I64 widened to f64) for ranking.
fn numeric_of(v: &TsValue) -> Option<f64> {
    match v {
        TsValue::F64(x) => Some(*x),
        TsValue::I64(x) => Some(*x as f64),
        _ => None,
    }
}

// Project the axis's columns for the rows in `order`, into a fresh frame at
// synthetic monotonic timestamps. Preserves each column's type and its null
// surface (a null cell is simply not pushed).
fn reorder_frame(axis: &RowAxis, order: &[usize]) -> TsDataFrame {
    let mut f = TsDataFrame::new();
    for (ci, name) in axis.names.iter().enumerate() {
        let col = build_group_column(&axis.columns[ci], order);
        f.push_column(name.clone(), col).unwrap();
    }
    f
}
