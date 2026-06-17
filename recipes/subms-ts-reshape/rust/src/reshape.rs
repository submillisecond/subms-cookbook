//! The reshape engine. A [`TsDataFrame`] is first flattened into a row-major
//! table of named, typed [`TsArray`]s over its union-of-timestamps row axis (the
//! same flattening `subms-ts-expr` and `subms-ts-join` do), then reshaped.
//! Output is a [`TsReshapeResult`]: ordered named [`TsArray`]s, all of equal
//! length, with Arrow-style validity for the missing cells a pivot / melt /
//! explode produces.
//!
//! The frame is per-column typed, so reshaping speaks real types. A pivot's
//! `columns_col` is usually a `Str` symbol (each distinct value names an output
//! column); a melt's `variable` column is a real `Str` column carrying the
//! source slot names; an explode walks a `Value` column of [`TsValue::Array`]
//! cells. None of these need a numeric encoding hack.

use std::collections::HashMap;

use subms_ts::{TsDataFrame, TsDataType, TsValue};
use subms_ts_expr::TsArray;

/// How [`pivot`] collapses the value cells that fall into a single (index,
/// column) bucket. `Last` keeps the value from the last source row in input
/// order; the rest are the obvious reductions over the bucket's present values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PivotAgg {
    Sum,
    Mean,
    Min,
    Max,
    Last,
}

/// Errors the reshape surface can raise. All are caller-input errors caught up
/// front; a successful call never partially fails.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsReshapeError {
    /// A named column is not present in the frame it was requested from.
    UnknownColumn { name: String },
    /// A reshape that needs at least one column to operate on got an empty
    /// column list (an empty `value_cols` for [`melt`], say).
    NoColumns,
    /// [`vstack`] and the row set-ops need both frames to carry the same column
    /// names in the same order; they did not.
    SchemaMismatch { a: Vec<String>, b: Vec<String> },
    /// [`hstack`] needs both frames to share a row axis (same row count); they
    /// did not.
    RowCountMismatch { a: usize, b: usize },
}

impl std::fmt::Display for TsReshapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsReshapeError::UnknownColumn { name } => write!(f, "unknown column: {name}"),
            TsReshapeError::NoColumns => write!(f, "reshape needs at least one column"),
            TsReshapeError::SchemaMismatch { a, b } => {
                write!(f, "schema mismatch: a={a:?}, b={b:?}")
            }
            TsReshapeError::RowCountMismatch { a, b } => {
                write!(f, "hstack row-count mismatch: a={a}, b={b}")
            }
        }
    }
}

impl std::error::Error for TsReshapeError {}

/// The result of a reshape: ordered named columns, all of the same length
/// ([`Self::nrows`]). Absent cells (an (index, column) pivot pair with no source
/// rows, or a melt over a column that was missing at a row) carry an unset
/// validity bit; read them with [`TsArray::get`] or coalesce with
/// [`TsArray::fill_null`]. The exact shape `subms-ts-join` / `subms-ts-groupby`
/// produce, so a reshape output drops straight back into the expression
/// evaluator.
#[derive(Clone, Debug, PartialEq)]
pub struct TsReshapeResult {
    columns: Vec<(String, TsArray)>,
    nrows: usize,
}

impl TsReshapeResult {
    fn new(columns: Vec<(String, TsArray)>, nrows: usize) -> Self {
        debug_assert!(columns.iter().all(|(_, c)| c.len() == nrows));
        Self { columns, nrows }
    }

    pub fn nrows(&self) -> usize {
        self.nrows
    }

    pub fn ncols(&self) -> usize {
        self.columns.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nrows == 0
    }

    /// The named columns in output order.
    pub fn columns(&self) -> &[(String, TsArray)] {
        &self.columns
    }

    /// Column names in output order.
    pub fn column_names(&self) -> impl Iterator<Item = &str> {
        self.columns.iter().map(|(n, _)| n.as_str())
    }

    /// A column by its output name.
    pub fn column(&self, name: &str) -> Option<&TsArray> {
        self.columns
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, c)| c)
    }

    /// A column by output index.
    pub fn column_at(&self, i: usize) -> Option<&TsArray> {
        self.columns.get(i).map(|(_, c)| c)
    }
}

// ---------- frame flattening ----------

/// Flatten a frame to named, typed [`TsArray`]s over its union-of-timestamps
/// row axis - exactly the shape `subms-ts-expr` evaluates against. Each column
/// is dense over the row axis: a row where the column had no point is a null
/// cell (validity unset). A `Value` column keeps its boxed [`TsValue`] cells in
/// a side table (see [`frame_value_cells`]) because [`TsArray`] has no Value
/// variant; everything else lands in a typed array. Exposed so a caller can
/// inspect the same flattening the reshape ops use.
pub fn frame_columns(frame: &TsDataFrame) -> Vec<(String, TsArray)> {
    flatten(frame).0
}

/// The per-row boxed cells of every `Value`-typed column in a frame, keyed by
/// column name. Used by [`explode`], whose list cells are [`TsValue::Array`]
/// documents that an f64 [`TsArray`] cannot carry.
pub fn frame_value_cells(frame: &TsDataFrame) -> ValueCells {
    flatten(frame).1
}

// Named, typed columns over the row axis - the shape every reshape op operates
// on, and what frame_columns returns.
type NamedColumns = Vec<(String, TsArray)>;
// The boxed per-row cells of every Value column, keyed by column name.
type ValueCells = HashMap<String, Vec<Option<TsValue>>>;

// One pass over the aligned rows, projecting each column to its typed array and
// keeping the raw boxed cells for Value columns on the side.
fn flatten(frame: &TsDataFrame) -> (NamedColumns, ValueCells) {
    let order: Vec<String> = frame.column_names().map(|s| s.to_string()).collect();
    let types: Vec<TsDataType> = order
        .iter()
        .map(|n| {
            frame
                .column(n)
                .map(|c| c.data_type())
                .unwrap_or(TsDataType::F64)
        })
        .collect();
    let mut cells: Vec<Vec<Option<TsValue>>> = vec![Vec::new(); order.len()];
    for (_ts, row) in frame.aligned() {
        for (i, cell) in row.into_iter().enumerate() {
            cells[i].push(cell);
        }
    }
    let mut value_cells = HashMap::new();
    let mut out = Vec::with_capacity(order.len());
    for ((name, ty), col_cells) in order.into_iter().zip(types).zip(cells) {
        if ty == TsDataType::Value {
            value_cells.insert(name.clone(), col_cells.clone());
        }
        out.push((name, cells_to_array(ty, &col_cells)));
    }
    (out, value_cells)
}

// Project a column's per-row Option<TsValue> cells onto a typed TsArray of the
// column's declared dtype. A cell whose boxed value does not match the dtype is
// treated as null (a stored column never mixes types, so this is defensive).
fn cells_to_array(ty: TsDataType, cells: &[Option<TsValue>]) -> TsArray {
    let n = cells.len();
    match ty {
        TsDataType::F64 | TsDataType::Value => {
            let mut values = vec![0.0; n];
            let mut valid = vec![false; n];
            for (i, c) in cells.iter().enumerate() {
                if let Some(v) = c.as_ref().and_then(value_as_f64) {
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

fn value_as_f64(v: &TsValue) -> Option<f64> {
    match v {
        TsValue::F64(x) => Some(*x),
        TsValue::I64(x) => Some(*x as f64),
        _ => None,
    }
}

fn nrows_of(columns: &[(String, TsArray)]) -> usize {
    columns.first().map(|(_, c)| c.len()).unwrap_or(0)
}

fn resolve(columns: &[(String, TsArray)], col: &str) -> Result<usize, TsReshapeError> {
    columns
        .iter()
        .position(|(n, _)| n == col)
        .ok_or_else(|| TsReshapeError::UnknownColumn {
            name: col.to_string(),
        })
}

// ---------- typed array builder ----------

// Accumulates typed cells (Some / None) into the matching TsArray variant. A
// cell whose boxed type disagrees with the builder's type is recorded as null.
// The same builder the join uses, so reshape outputs share a wire shape.
enum ArrayBuilder {
    F64 { values: Vec<f64>, valid: Vec<bool> },
    I64 { values: Vec<i64>, valid: Vec<bool> },
    Bool { values: Vec<bool>, valid: Vec<bool> },
    Str { values: Vec<String>, valid: Vec<bool> },
}

impl ArrayBuilder {
    fn for_type(ty: TsDataType, cap: usize) -> ArrayBuilder {
        match ty {
            TsDataType::I64 => ArrayBuilder::I64 {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
            TsDataType::Bool => ArrayBuilder::Bool {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
            TsDataType::Str => ArrayBuilder::Str {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
            // F64 and the schemaless Value escape hatch both land in an f64
            // array, matching the flattening in cells_to_array.
            TsDataType::F64 | TsDataType::Value => ArrayBuilder::F64 {
                values: Vec::with_capacity(cap),
                valid: Vec::with_capacity(cap),
            },
        }
    }

    fn push(&mut self, cell: Option<TsValue>) {
        match self {
            ArrayBuilder::F64 { values, valid } => match cell.as_ref().and_then(value_as_f64) {
                Some(f) => {
                    values.push(f);
                    valid.push(true);
                }
                None => {
                    values.push(0.0);
                    valid.push(false);
                }
            },
            ArrayBuilder::I64 { values, valid } => match cell {
                Some(TsValue::I64(v)) => {
                    values.push(v);
                    valid.push(true);
                }
                _ => {
                    values.push(0);
                    valid.push(false);
                }
            },
            ArrayBuilder::Bool { values, valid } => match cell {
                Some(TsValue::Bool(v)) => {
                    values.push(v);
                    valid.push(true);
                }
                _ => {
                    values.push(false);
                    valid.push(false);
                }
            },
            ArrayBuilder::Str { values, valid } => match cell {
                Some(TsValue::Str(v)) => {
                    values.push(v);
                    valid.push(true);
                }
                _ => {
                    values.push(String::new());
                    valid.push(false);
                }
            },
        }
    }

    fn finish(self) -> TsArray {
        match self {
            ArrayBuilder::F64 { values, valid } => TsArray::F64 { values, valid },
            ArrayBuilder::I64 { values, valid } => TsArray::I64 { values, valid },
            ArrayBuilder::Bool { values, valid } => TsArray::Bool { values, valid },
            ArrayBuilder::Str { values, valid } => TsArray::Str { values, valid },
        }
    }
}

// Pick the array variant that can hold a heterogeneous set of source columns: a
// single shared dtype if they all agree, else Value/f64 is too lossy, so we
// fall back to a Str representation of each cell. melt's value column uses this.
fn common_type(types: &[TsDataType]) -> Option<TsDataType> {
    let first = *types.first()?;
    if types.iter().all(|&t| t == first) {
        Some(first)
    } else {
        None
    }
}

// ---------- pivot ----------

// A running bucket accumulator. Keeps everything an agg might need so the same
// state serves Sum / Mean / Min / Max / Last without re-scanning.
#[derive(Clone, Copy)]
struct Acc {
    sum: f64,
    count: u64,
    min: f64,
    max: f64,
    last: f64,
}

impl Acc {
    fn seed(v: f64) -> Acc {
        Acc {
            sum: v,
            count: 1,
            min: v,
            max: v,
            last: v,
        }
    }

    fn fold(&mut self, v: f64) {
        self.sum += v;
        self.count += 1;
        if v < self.min {
            self.min = v;
        }
        if v > self.max {
            self.max = v;
        }
        self.last = v;
    }

    fn finish(&self, agg: PivotAgg) -> f64 {
        match agg {
            PivotAgg::Sum => self.sum,
            PivotAgg::Mean => self.sum / self.count as f64,
            PivotAgg::Min => self.min,
            PivotAgg::Max => self.max,
            PivotAgg::Last => self.last,
        }
    }
}

// The identity token for an index / category cell, type-tagged so a Str "3"
// never collides with a numeric 3. Ordered so the category axis is laid out
// deterministically.
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum CellKey {
    Bool(bool),
    I64(i64),
    F64Bits(u64),
    Str(String),
}

fn cell_key(arr: &TsArray, row: usize) -> Option<CellKey> {
    match arr.get(row)? {
        TsValue::Bool(b) => Some(CellKey::Bool(b)),
        TsValue::I64(v) => Some(CellKey::I64(v)),
        TsValue::F64(v) => Some(CellKey::F64Bits(v.to_bits())),
        TsValue::Str(s) => Some(CellKey::Str(s)),
        _ => None,
    }
}

// The output column NAME for a category value. A Str category names the column
// by its own text; a numeric one stringifies (integral without a decimal point
// so "3", not "3.0"). Mirrors the Java side exactly so pivot column names are
// byte-equivalent across the ports.
fn category_name(v: &TsValue) -> String {
    match v {
        TsValue::Str(s) => s.clone(),
        TsValue::Bool(b) => b.to_string(),
        TsValue::I64(n) => n.to_string(),
        TsValue::F64(f) => format_f64(*f),
        _ => String::new(),
    }
}

fn format_f64(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

/// Long-to-wide pivot. Rows are keyed by the distinct values of `index_col`;
/// one output column is produced per distinct value of `columns_col`, named by
/// that value (a `Str` category names the column by its own text; a numeric one
/// stringifies). Each cell is the `agg` of `values_col` over the rows matching
/// that (index, column) pair.
///
/// The output carries the index column first (typed, name = `index_col`), then
/// one column per distinct category in ascending category order. Distinct index
/// values are emitted in first-seen (input) order, which is deterministic. An
/// (index, column) pair with no source rows is an absent cell: validity unset,
/// not a zero. Rows where the index, the column, or the value cell is missing
/// are skipped (a missing key cannot bucket). The value aggregates are always
/// `F64` columns (the agg reduces to a number); the index column keeps its own
/// type.
pub fn pivot(
    frame: &TsDataFrame,
    index_col: &str,
    columns_col: &str,
    values_col: &str,
    agg: PivotAgg,
) -> Result<TsReshapeResult, TsReshapeError> {
    let columns = frame_columns(frame);
    let idx_i = resolve(&columns, index_col)?;
    let col_i = resolve(&columns, columns_col)?;
    let val_i = resolve(&columns, values_col)?;
    let nrows = nrows_of(&columns);

    let idx = &columns[idx_i].1;
    let cat = &columns[col_i].1;
    let val = &columns[val_i].1;

    // Distinct index values in first-seen order, with a lookup to their output
    // row. The boxed value drives the typed index output column.
    let mut index_order: Vec<TsValue> = Vec::new();
    let mut index_row: HashMap<CellKey, usize> = HashMap::new();
    // Distinct category values, keyed for dedup; the boxed value drives the
    // output column name + the deterministic ascending sort.
    let mut category_set: HashMap<CellKey, TsValue> = HashMap::new();
    // (out_row, category_key) -> accumulator.
    let mut buckets: HashMap<(usize, CellKey), Acc> = HashMap::new();

    for r in 0..nrows {
        let (ikey, ival, ckey, cval, vv) = match (
            cell_key(idx, r),
            idx.get(r),
            cell_key(cat, r),
            cat.get(r),
            val.get(r).and_then(|v| value_as_f64(&v)),
        ) {
            (Some(ik), Some(iv), Some(ck), Some(cv), Some(v)) => (ik, iv, ck, cv, v),
            _ => continue,
        };
        let out_row = match index_row.get(&ikey) {
            Some(&row) => row,
            None => {
                let row = index_order.len();
                index_order.push(ival);
                index_row.insert(ikey, row);
                row
            }
        };
        category_set.entry(ckey.clone()).or_insert(cval);
        buckets
            .entry((out_row, ckey))
            .and_modify(|a| a.fold(vv))
            .or_insert_with(|| Acc::seed(vv));
    }

    // Categories sorted by their key ascending for a fully deterministic layout.
    let mut categories: Vec<(CellKey, TsValue)> = category_set.into_iter().collect();
    categories.sort_by(|a, b| a.0.cmp(&b.0));

    let out_rows = index_order.len();
    let mut out: Vec<(String, TsArray)> = Vec::with_capacity(1 + categories.len());

    // The index column, typed, fully valid (every emitted row had an index).
    let mut idx_builder = ArrayBuilder::for_type(idx.data_type(), out_rows);
    for v in &index_order {
        idx_builder.push(Some(v.clone()));
    }
    out.push((index_col.to_string(), idx_builder.finish()));

    for (ckey, cval) in &categories {
        let mut builder = ArrayBuilder::for_type(TsDataType::F64, out_rows);
        for out_row in 0..out_rows {
            let cell = buckets
                .get(&(out_row, ckey.clone()))
                .map(|a| TsValue::F64(a.finish(agg)));
            builder.push(cell);
        }
        out.push((category_name(cval), builder.finish()));
    }

    Ok(TsReshapeResult::new(out, out_rows))
}

// ---------- melt / unpivot ----------

/// Wide-to-long unpivot. For each input row x each `value_col`, emit one output
/// row of `{ id_cols..., variable, value }`. `variable` is a real `Str` column
/// carrying the NAME of the value column the cell came from; `value` carries the
/// cell. The id columns repeat across the value columns for a given input row.
///
/// `value`'s type is the shared dtype of every `value_col` when they agree; when
/// they mix, the value column is `Str` (each cell stringified) so the long form
/// stays a single typed column. A missing source cell becomes a null in the
/// `value` column (validity unset). Output row order is input-row-major, then
/// value-column order within a row, which is deterministic.
///
/// This is the headline capability the typed frame unlocks: the `variable`
/// column is a genuine string column the old f64-only frame could not hold.
pub fn melt(
    frame: &TsDataFrame,
    id_cols: &[&str],
    value_cols: &[&str],
) -> Result<TsReshapeResult, TsReshapeError> {
    if value_cols.is_empty() {
        return Err(TsReshapeError::NoColumns);
    }
    let columns = frame_columns(frame);
    let nrows = nrows_of(&columns);

    let id_idx: Vec<usize> = id_cols
        .iter()
        .map(|c| resolve(&columns, c))
        .collect::<Result<_, _>>()?;
    let val_idx: Vec<usize> = value_cols
        .iter()
        .map(|c| resolve(&columns, c))
        .collect::<Result<_, _>>()?;

    let out_rows = nrows * value_cols.len();

    // The value column's type: the shared dtype if every value column agrees,
    // else Str (mixed types collapse to their string rendering).
    let val_types: Vec<TsDataType> = val_idx.iter().map(|&i| columns[i].1.data_type()).collect();
    let value_type = common_type(&val_types).unwrap_or(TsDataType::Str);

    // id columns, each repeated value_cols.len() times per input row.
    let mut out: Vec<(String, TsArray)> = Vec::with_capacity(id_idx.len() + 2);
    for &ci in &id_idx {
        let src = &columns[ci].1;
        let mut builder = ArrayBuilder::for_type(src.data_type(), out_rows);
        for r in 0..nrows {
            let cell = src.get(r);
            for _ in 0..value_cols.len() {
                builder.push(cell.clone());
            }
        }
        out.push((columns[ci].0.clone(), builder.finish()));
    }

    // the variable column: a real Str column naming the source value column.
    let mut var_values = Vec::with_capacity(out_rows);
    let var_valid = vec![true; out_rows];
    for _ in 0..nrows {
        for &vi in &val_idx {
            var_values.push(columns[vi].0.clone());
        }
    }
    out.push((
        "variable".to_string(),
        TsArray::Str {
            values: var_values,
            valid: var_valid,
        },
    ));

    // the value column: the source cell, coerced to value_type.
    let mut val_builder = ArrayBuilder::for_type(value_type, out_rows);
    for r in 0..nrows {
        for &vi in &val_idx {
            let cell = columns[vi].1.get(r);
            let coerced = match value_type {
                TsDataType::Str => cell.map(stringify_cell).map(TsValue::Str),
                _ => cell,
            };
            val_builder.push(coerced);
        }
    }
    out.push(("value".to_string(), val_builder.finish()));

    Ok(TsReshapeResult::new(out, out_rows))
}

fn stringify_cell(v: TsValue) -> String {
    match v {
        TsValue::Str(s) => s,
        TsValue::Bool(b) => b.to_string(),
        TsValue::I64(n) => n.to_string(),
        TsValue::F64(f) => format_f64(f),
        other => format!("{other:?}"),
    }
}

// ---------- explode ----------

/// Explode a `Value` column of [`TsValue::Array`] cells: each list cell expands
/// to one output row per element, with every other column's cell repeated for
/// each element. A row whose `list_col` cell is an EMPTY array (or a null /
/// non-array cell) is DROPPED - it contributes no output rows, matching Polars
/// `explode` on an empty list and DuckDB `UNNEST` (which drops empty lists).
///
/// The exploded column is emitted as a `Value` column flattened into an f64
/// [`TsArray`] when its elements are numeric, else a `Str` array of the
/// elements' string rendering; the other columns keep their own type. Output
/// row order is input-row-major, then element order within a row. Deterministic.
///
/// `list_col` must be a `Value` column; a non-Value column has no list cells to
/// explode, so the result is empty (every row drops).
pub fn explode(frame: &TsDataFrame, list_col: &str) -> Result<TsReshapeResult, TsReshapeError> {
    let columns = frame_columns(frame);
    let list_i = resolve(&columns, list_col)?;
    let nrows = nrows_of(&columns);
    let value_cells = frame_value_cells(frame);

    // The per-row element lists. A non-Value column, a null cell, or a non-array
    // cell yields an empty list, which drops the row.
    let list_cells = value_cells.get(list_col);
    let mut row_elems: Vec<Vec<TsValue>> = Vec::with_capacity(nrows);
    for r in 0..nrows {
        let elems = match list_cells.and_then(|cells| cells.get(r)).and_then(|c| c.as_ref()) {
            Some(TsValue::Array(items)) => items.clone(),
            _ => Vec::new(),
        };
        row_elems.push(elems);
    }

    let out_rows: usize = row_elems.iter().map(|e| e.len()).sum();

    // The exploded column's element type: f64 when every emitted element is
    // numeric, else Str. Decided once over all elements for a stable column type.
    let all_numeric = row_elems
        .iter()
        .flatten()
        .all(|e| value_as_f64(e).is_some());
    let exploded_type = if all_numeric {
        TsDataType::F64
    } else {
        TsDataType::Str
    };

    let mut out: Vec<(String, TsArray)> = Vec::with_capacity(columns.len());
    for (ci, (name, src)) in columns.iter().enumerate() {
        if ci == list_i {
            let mut builder = ArrayBuilder::for_type(exploded_type, out_rows);
            for elems in &row_elems {
                for e in elems {
                    let cell = match exploded_type {
                        TsDataType::Str => Some(TsValue::Str(stringify_cell(e.clone()))),
                        _ => value_as_f64(e).map(TsValue::F64),
                    };
                    builder.push(cell);
                }
            }
            out.push((name.clone(), builder.finish()));
        } else {
            let mut builder = ArrayBuilder::for_type(src.data_type(), out_rows);
            for (r, elems) in row_elems.iter().enumerate() {
                let cell = src.get(r);
                for _ in 0..elems.len() {
                    builder.push(cell.clone());
                }
            }
            out.push((name.clone(), builder.finish()));
        }
    }

    Ok(TsReshapeResult::new(out, out_rows))
}

// ---------- concatenation ----------

fn schema_names(columns: &[(String, TsArray)]) -> Vec<String> {
    columns.iter().map(|(n, _)| n.clone()).collect()
}

/// Row concatenation: every row of `a` followed by every row of `b`. Both
/// frames must carry the same column names in the same order; otherwise
/// [`TsReshapeError::SchemaMismatch`]. The output schema is that shared schema,
/// and a cell that was missing in either input stays missing.
pub fn vstack(a: &TsDataFrame, b: &TsDataFrame) -> Result<TsReshapeResult, TsReshapeError> {
    let a_cols = frame_columns(a);
    let b_cols = frame_columns(b);
    let a_names = schema_names(&a_cols);
    let b_names = schema_names(&b_cols);
    if a_names != b_names {
        return Err(TsReshapeError::SchemaMismatch {
            a: a_names,
            b: b_names,
        });
    }
    let a_rows = nrows_of(&a_cols);
    let b_rows = nrows_of(&b_cols);
    let total = a_rows + b_rows;

    let mut out: Vec<(String, TsArray)> = Vec::with_capacity(a_cols.len());
    for ((name, ac), (_, bc)) in a_cols.iter().zip(b_cols.iter()) {
        let mut builder = ArrayBuilder::for_type(ac.data_type(), total);
        for r in 0..a_rows {
            builder.push(ac.get(r));
        }
        for r in 0..b_rows {
            builder.push(bc.get(r));
        }
        out.push((name.clone(), builder.finish()));
    }
    Ok(TsReshapeResult::new(out, total))
}

/// Column concatenation: the columns of `a` followed by the columns of `b`,
/// over a shared row axis. Both frames must have the same row count; otherwise
/// [`TsReshapeError::RowCountMismatch`]. A column name carried by both inputs is
/// suffixed `_a` / `_b` so the output names stay unique.
pub fn hstack(a: &TsDataFrame, b: &TsDataFrame) -> Result<TsReshapeResult, TsReshapeError> {
    let a_cols = frame_columns(a);
    let b_cols = frame_columns(b);
    let a_rows = nrows_of(&a_cols);
    let b_rows = nrows_of(&b_cols);
    if a_rows != b_rows {
        return Err(TsReshapeError::RowCountMismatch {
            a: a_rows,
            b: b_rows,
        });
    }
    let a_names = schema_names(&a_cols);
    let b_names = schema_names(&b_cols);

    let mut out: Vec<(String, TsArray)> = Vec::with_capacity(a_cols.len() + b_cols.len());
    for (name, col) in &a_cols {
        let out_name = if b_names.contains(name) {
            format!("{name}_a")
        } else {
            name.clone()
        };
        out.push((out_name, col.clone()));
    }
    for (name, col) in &b_cols {
        let out_name = if a_names.contains(name) {
            format!("{name}_b")
        } else {
            name.clone()
        };
        out.push((out_name, col.clone()));
    }
    Ok(TsReshapeResult::new(out, a_rows))
}

// ---------- row set-ops ----------

// A row encoded for set membership: the per-cell typed token (Some / None).
// Equality and hashing are on this tuple, so two rows are the same row iff every
// cell's typed token (or missing-ness) agrees. f64 compares by bit pattern, Str
// by value - so a Str "3" never equals a numeric 3.
#[derive(Clone, PartialEq, Eq, Hash)]
struct RowKey(Vec<Option<CellKey>>);

fn row_key(columns: &[(String, TsArray)], r: usize) -> RowKey {
    RowKey(columns.iter().map(|(_, c)| cell_key(c, r)).collect())
}

// A (source columns, row index) pick for a set-op output, in pick order.
type Pick<'a> = (&'a [(String, TsArray)], usize);

// Build the output of a set-op from a sequence of picks, under the shared schema
// of `template`.
fn assemble_rows(template: &[(String, TsArray)], picks: &[Pick<'_>]) -> TsReshapeResult {
    let mut builders: Vec<ArrayBuilder> = template
        .iter()
        .map(|(_, c)| ArrayBuilder::for_type(c.data_type(), picks.len()))
        .collect();
    for &(cols, r) in picks {
        for (ci, builder) in builders.iter_mut().enumerate() {
            builder.push(cols[ci].1.get(r));
        }
    }
    let columns = template
        .iter()
        .zip(builders)
        .map(|((name, _), b)| (name.clone(), b.finish()))
        .collect();
    TsReshapeResult::new(columns, picks.len())
}

fn require_same_schema(
    a_names: &[String],
    b_names: &[String],
) -> Result<(), TsReshapeError> {
    if a_names != b_names {
        return Err(TsReshapeError::SchemaMismatch {
            a: a_names.to_vec(),
            b: b_names.to_vec(),
        });
    }
    Ok(())
}

/// Distinct rows present in either `a` or `b`, treating each row as a typed cell
/// tuple. Both frames must share the same schema. Output order: distinct rows of
/// `a` in input order, then rows of `b` not already seen, in input order.
/// Deterministic.
pub fn union(a: &TsDataFrame, b: &TsDataFrame) -> Result<TsReshapeResult, TsReshapeError> {
    let a_cols = frame_columns(a);
    let b_cols = frame_columns(b);
    require_same_schema(&schema_names(&a_cols), &schema_names(&b_cols))?;

    let mut seen: std::collections::HashSet<RowKey> = std::collections::HashSet::new();
    let mut picks: Vec<Pick<'_>> = Vec::new();
    for r in 0..nrows_of(&a_cols) {
        if seen.insert(row_key(&a_cols, r)) {
            picks.push((a_cols.as_slice(), r));
        }
    }
    for r in 0..nrows_of(&b_cols) {
        if seen.insert(row_key(&b_cols, r)) {
            picks.push((b_cols.as_slice(), r));
        }
    }
    Ok(assemble_rows(&a_cols, &picks))
}

/// Distinct rows present in BOTH `a` and `b`. Both frames must share the same
/// schema. Output order: the qualifying distinct rows of `a` in input order.
/// Deterministic.
pub fn intersect(a: &TsDataFrame, b: &TsDataFrame) -> Result<TsReshapeResult, TsReshapeError> {
    let a_cols = frame_columns(a);
    let b_cols = frame_columns(b);
    require_same_schema(&schema_names(&a_cols), &schema_names(&b_cols))?;

    let mut b_set: std::collections::HashSet<RowKey> = std::collections::HashSet::new();
    for r in 0..nrows_of(&b_cols) {
        b_set.insert(row_key(&b_cols, r));
    }
    let mut emitted: std::collections::HashSet<RowKey> = std::collections::HashSet::new();
    let mut picks: Vec<Pick<'_>> = Vec::new();
    for r in 0..nrows_of(&a_cols) {
        let key = row_key(&a_cols, r);
        if b_set.contains(&key) && emitted.insert(key) {
            picks.push((a_cols.as_slice(), r));
        }
    }
    Ok(assemble_rows(&a_cols, &picks))
}

/// Distinct rows present in `a` but NOT in `b`. Both frames must share the same
/// schema. Output order: the qualifying distinct rows of `a` in input order.
/// Deterministic.
pub fn except(a: &TsDataFrame, b: &TsDataFrame) -> Result<TsReshapeResult, TsReshapeError> {
    let a_cols = frame_columns(a);
    let b_cols = frame_columns(b);
    require_same_schema(&schema_names(&a_cols), &schema_names(&b_cols))?;

    let mut b_set: std::collections::HashSet<RowKey> = std::collections::HashSet::new();
    for r in 0..nrows_of(&b_cols) {
        b_set.insert(row_key(&b_cols, r));
    }
    let mut emitted: std::collections::HashSet<RowKey> = std::collections::HashSet::new();
    let mut picks: Vec<Pick<'_>> = Vec::new();
    for r in 0..nrows_of(&a_cols) {
        let key = row_key(&a_cols, r);
        if !b_set.contains(&key) && emitted.insert(key) {
            picks.push((a_cols.as_slice(), r));
        }
    }
    Ok(assemble_rows(&a_cols, &picks))
}
