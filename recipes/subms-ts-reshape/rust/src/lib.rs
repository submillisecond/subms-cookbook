//! `subms-ts-reshape` - the reshape half of the Polars / DuckDB relational
//! surface over a heterogeneous [`TsDataFrame`](subms_ts::TsDataFrame): the
//! long-to-wide [`pivot`], the wide-to-long [`melt`] (unpivot), the list-cell
//! [`explode`], the [`vstack`] / [`hstack`] concatenators, and the row set-ops
//! [`union`] / [`intersect`] / [`except`].
//!
//! Every operation flattens its input(s) to dense named, typed
//! [`TsArray`](subms_ts_expr::TsArray)s over the union-of-timestamps row axis
//! (the same flattening `subms-ts-expr`, `subms-ts-join`, and `subms-ts-groupby`
//! use), reshapes, and returns a [`TsReshapeResult`] of named [`TsArray`]s - so
//! the output drops straight back into the expression evaluator.
//!
//! # The frame is per-column typed, so reshaping speaks types
//!
//! [`TsDataFrame`](subms_ts::TsDataFrame) holds a `Str` symbol column, an `I64`
//! date column, a `Bool` flag, an `F64` reading, and a schemaless `Value`
//! document column - each as its own typed slot. That unlocks the full reshape
//! surface:
//!
//! - [`pivot`] takes the DISTINCT values of `columns_col` - typically a `Str`
//!   symbol - and turns each into an output column named by the value. The cells
//!   are numeric aggregates (Sum / Mean / Min / Max / Last).
//! - [`melt`] is wide-to-long: it emits a real `Str` `"variable"` column naming
//!   the source slot per row, plus a `value` column. The string variable column
//!   is the capability the old f64-only frame could not express; it is now a
//!   first-class claim.
//! - [`explode`] walks a `Value` column of [`TsValue::Array`](subms_ts::TsValue)
//!   cells and emits one row per element, repeating the other columns. An empty
//!   list drops the row (Polars / DuckDB `UNNEST` semantics).
//!
//! # Absent cells are validity bits, not sentinels
//!
//! A pivot's output grid is (distinct index) x (distinct column). An (index,
//! column) pair with no source rows has no aggregate, so its cell is MISSING -
//! `valid[i] = false` in the [`TsArray`](subms_ts_expr::TsArray) Arrow-style
//! model, not a zero. A melt over a column that was absent at a row is likewise
//! null. A caller reads it with [`TsArray::get`](subms_ts_expr::TsArray::get)
//! (`None`) or coalesces with
//! [`TsArray::fill_null`](subms_ts_expr::TsArray::fill_null).
//!
//! # Row equality is the typed cell tuple
//!
//! The set-ops ([`union`] / [`intersect`] / [`except`]) treat each row as a
//! tuple of typed cells: `F64` compares by bit pattern (`f64::to_bits`), `Str`
//! by value, with a missing cell as its own distinct token. So a `Str` `"3"`
//! never equals a numeric `3`, `-0.0` and `+0.0` are DISTINCT rows, and two
//! `NaN`s with the same bits ARE equal - a documented consequence of bit
//! equality, not a fuzzy compare.
//!
//! # Contract
//!
//! Throughput-contracted, NOT per-op sub-ms. Reshaping is a whole-frame
//! operation; the honest number is rows/sec, captured in `perf/{rust,java}.json`.
//! The bench asserts only a generous no-pathological-stall guard, not a tight
//! p99.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries};
//! use subms_ts_reshape::{melt, MeltColumns};
//!
//! // wide form: one row per day, an "open" and a "close" column. Melt to long.
//! let mut day = TsSeries::<i64>::new();
//! let mut open = TsSeries::<f64>::new();
//! let mut close = TsSeries::<f64>::new();
//! for (i, (d, o, c)) in [(0i64, 10.0, 11.0), (1, 20.0, 22.0)].into_iter().enumerate() {
//!     day.push(i as i64, d).unwrap();
//!     open.push(i as i64, o).unwrap();
//!     close.push(i as i64, c).unwrap();
//! }
//! let f = TsDataFrame::new()
//!     .with_column("day", TsColumn::I64(day))
//!     .with_column("open", TsColumn::F64(open))
//!     .with_column("close", TsColumn::F64(close));
//!
//! let out = melt(&f, &["day"], &["open", "close"]).unwrap();
//! // 2 rows x 2 value cols = 4 long rows; the "variable" column names the slot.
//! assert_eq!(out.nrows(), 4);
//! let var = out.column("variable").unwrap();
//! assert_eq!(var.get(0), Some(subms_ts::TsValue::Str("open".into())));
//! assert_eq!(var.get(1), Some(subms_ts::TsValue::Str("close".into())));
//! ```
//!
//! [`MeltColumns`] is a doc alias for the `(id_cols, value_cols)` argument pair
//! to keep the signature legible; it carries no runtime cost.

mod reshape;

pub use reshape::{
    PivotAgg, TsReshapeError, TsReshapeResult, except, explode, frame_columns, frame_value_cells,
    hstack, intersect, melt, pivot, union, vstack,
};

/// Doc-only alias for the `(id_cols, value_cols)` pair [`melt`] takes - the id
/// columns that repeat down the long form and the value columns that unpivot.
pub type MeltColumns<'a> = (&'a [&'a str], &'a [&'a str]);

// Re-export the result array so a downstream caller doesn't have to also depend
// on subms-ts-expr just to read a reshaped output's cells.
pub use subms_ts_expr::TsArray;

#[cfg(feature = "harness")]
pub mod recipe;
