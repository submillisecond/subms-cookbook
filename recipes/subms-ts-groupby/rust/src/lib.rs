//! `subms-ts-groupby` - typed, multi-key group-by with multi-aggregation over a
//! heterogeneous [`TsDataFrame`], the defining Polars / DuckDB operation. The
//! key columns may be ANY typed column - a `Str` symbol, an `I64` id / date, an
//! `F64`, a `Bool` - and the group key is the TUPLE of those typed cells. Each
//! aggregation is an ordinary [`TsExpr`] reduced PER GROUP via `eval_scalar`
//! over that group's sub-frame.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
//! use subms_ts_expr::TsExpr;
//! use subms_ts_groupby::group_by;
//!
//! // a symbol column (Str key) + a size column.
//! let mut symbol = TsSeries::<String>::new();
//! let mut size = TsSeries::<f64>::new();
//! for (i, (sym, sz)) in [("AAPL", 10.0), ("MSFT", 5.0), ("AAPL", 7.0), ("MSFT", 3.0)]
//!     .into_iter()
//!     .enumerate()
//! {
//!     symbol.push(i as i64, sym.to_string()).unwrap();
//!     size.push(i as i64, sz).unwrap();
//! }
//! let f = TsDataFrame::new()
//!     .with_column("symbol", TsColumn::Str(symbol))
//!     .with_column("size", TsColumn::F64(size));
//!
//! let result = group_by(&f, &["symbol"])
//!     .unwrap()
//!     .agg(&[
//!         ("total_size", TsExpr::col("size").sum()),
//!         ("n", TsExpr::col("size").count()),
//!     ])
//!     .unwrap();
//!
//! // one row per symbol, sorted by key: AAPL -> 17, MSFT -> 8.
//! assert_eq!(result.nrows(), 2);
//! assert_eq!(result.value("symbol", 0), Some(TsValue::Str("AAPL".to_string())));
//! assert_eq!(result.value("total_size", 0), Some(TsValue::F64(17.0)));
//! assert_eq!(result.value("total_size", 1), Some(TsValue::F64(8.0)));
//! ```
//!
//! This recipe is the analytical front: it is THROUGHPUT-contracted, not a
//! per-op sub-ms primitive. A group-by-plus-aggregate over a few thousand rows
//! lands in tens to low hundreds of microseconds; the honest number to read is
//! rows/sec, captured in `perf/`. See the bench for the deliberate non-claim.

mod groupby;

pub use groupby::{
    GroupByError, TsGroupBy, TsGroupResult, group_by, sort_by, top_k, unique, value_counts,
};

#[cfg(feature = "harness")]
pub mod recipe;
