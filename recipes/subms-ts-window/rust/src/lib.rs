//! `subms-ts-window` - SQL-style window functions over a heterogeneous
//! [`TsDataFrame`](subms_ts::TsDataFrame), partitioned by ANY typed key column.
//! Each function partitions the frame's union-of-timestamps rows by a TUPLE of
//! TYPED key cells (a `Str` symbol, an `I64` venue id, an `F64`, a `Bool`),
//! orders the rows inside each partition by an order-by column (default: the
//! frame's row/ts order), then applies a per-partition transform and scatters
//! the result back onto the original row axis. The output is a typed
//! [`TsArray`](subms_ts_expr::TsArray) the same length as the frame's aligned
//! row axis - exactly what `subms-ts-expr`'s `eval` produces, so a window output
//! composes straight back into further expression evaluation as a derived
//! column.
//!
//! The shapes mirror SQL's `OVER (PARTITION BY k ORDER BY o)`:
//!
//! - [`lag`] / [`lead`] - shift a column within each partition by `n` rows;
//!   cells that fall off the partition head/tail are null. Works for ANY column
//!   type - the result array matches the input column's type.
//! - [`row_number`] - 1..=k position inside each partition (`I64`).
//! - [`rank`] / [`dense_rank`] - order-sensitive ranks; ties share a rank
//!   (`I64`).
//! - [`cumsum`] / [`cumprod`] / [`cummin`] / [`cummax`] - running reductions
//!   inside each partition, in order-by order (numeric `F64`/`I64` input, `F64`
//!   result).
//! - [`over`] - evaluate a [`TsExpr`](subms_ts_expr::TsExpr) aggregation over
//!   each partition's sub-frame and broadcast the scalar back to every row of
//!   that partition (the SQL `agg() OVER (PARTITION BY k)` shape).
//!
//! ## Typed partition keys
//!
//! The partition key is the TUPLE of the partition columns' TYPED cells at a
//! row. Partitioning by a `Str` symbol column is the headline case - a `lag`
//! over `px` partitioned by `symbol` is the per-symbol previous price, with no
//! pre-encoding step. (A high-cardinality string key can be pre-encoded to a
//! `u32` code via `subms-ts-categorical` upstream; this recipe takes no
//! categorical dependency.)
//!
//! ## Validity model
//!
//! Window outputs carry a validity bitmap for undefined cells. A `lag(1)` at a
//! partition's first row has no predecessor, so that cell is invalid, not zero.
//! A running reduction whose input cell is itself invalid skips that cell (the
//! running state carries forward; the output is invalid only until a valid input
//! has been folded in). This is the DERIVED-null model `subms-ts-expr` uses,
//! distinct from `TsSeries`' no-null-on-ingest invariant.
//!
//! ## Contract
//!
//! This recipe is THROUGHPUT-contracted, not per-op sub-ms. A window pass is an
//! analytical-front operation (partition, sort, scan), not a tick-loop op. The
//! bench measures whole-frame window passes over a partitioned 4,096-row frame
//! and asserts only a generous no-pathological-stall guard (`over`, the heavy
//! stage, sits near 17 ms p99). The honest number to read is throughput in
//! `perf/rust.json`.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
//! use subms_ts_window::lag;
//!
//! // two symbols interleaved by ts: AAPL at even ts, MSFT at odd ts.
//! let mut sym = TsSeries::<String>::new();
//! let mut px = TsSeries::<f64>::new();
//! for i in 0..6 {
//!     let s = if i % 2 == 0 { "AAPL" } else { "MSFT" };
//!     sym.push(i, s.to_string()).unwrap();
//!     px.push(i, i as f64).unwrap();
//! }
//! let f = TsDataFrame::new()
//!     .with_column("sym", TsColumn::Str(sym))
//!     .with_column("px", TsColumn::F64(px));
//!
//! // previous price within each symbol - partitioned on a STRING key.
//! let prev = lag(&f, "px", 1, &["sym"]).unwrap();
//! assert_eq!(prev.get(0), None); // AAPL ts 0: no predecessor
//! assert_eq!(prev.get(1), None); // MSFT ts 1: no predecessor
//! assert_eq!(prev.get(2), Some(TsValue::F64(0.0))); // AAPL ts 2 -> px at ts 0
//! assert_eq!(prev.get(3), Some(TsValue::F64(1.0))); // MSFT ts 3 -> px at ts 1
//! ```

mod window;

pub use window::{
    TsWindowError, cummax, cummin, cumprod, cumsum, dense_rank, lag, lead, over, rank, row_number,
};

// Re-exported for downstream callers that want the result array type without
// also pulling subms-ts-expr directly.
pub use subms_ts_expr::TsArray;

#[cfg(feature = "harness")]
pub mod recipe;
