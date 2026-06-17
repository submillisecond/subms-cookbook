//! `subms-ts-join` - the full equi-join matrix between two heterogeneous
//! [`TsDataFrame`](subms_ts::TsDataFrame)s on key column(s) of ANY type. Three
//! execution strategies, six join kinds, one output type.
//!
//! - [`hash_join`] builds a hash table on one input and probes with the other -
//!   the workhorse for unsorted keys.
//! - [`sort_merge_join`] sorts both inputs on the key tuple and merges in a
//!   single linear pass - wins when inputs arrive sorted (the common time-series
//!   case) or when the join is many-to-many and the hash table would balloon.
//! - [`cross_join`] is the cartesian product, no keys.
//!
//! Six kinds via [`TsJoinKind`]: `Inner`, `Left`, `Right`, `Outer`, `Semi`,
//! `Anti`. Hash and sort-merge produce the same result SET for every kind; only
//! the row ORDER differs in the many-to-many case, and both are deterministic
//! and documented (see [`hash_join`] / [`sort_merge_join`]).
//!
//! # Keys are typed cells, not f64
//!
//! A real join keys on a `Str` symbol, an `I64` date, a `Bool` flag, or any
//! tuple mixing them - not just `f64`. A key cell carries its type into the
//! key token, so a `Str` "AAPL" only ever joins another `Str` "AAPL", never a
//! numeric column that happens to share bits. Multi-key joins pair the key
//! lists positionally: `&["sym", "date"]` against `&["symbol", "session"]`
//! joins `sym==symbol AND date==session`.
//!
//! # Nulls are validity bits, not sentinels
//!
//! An outer / left / right join emits rows where one side has no match. The
//! unmatched side's columns are MISSING there, not zero. We use
//! [`subms_ts_expr::TsArray`]'s Arrow-style validity model: a missing cell sets
//! `valid[i] = false` and leaves `values[i]` unspecified. The output is a
//! [`TsJoinResult`] of named [`TsArray`]s, every one the same length (the row
//! count of the join), and a caller reads a cell with [`TsArray::get`] (`None`
//! on a missing cell) or coalesces it with [`TsArray::fill_null`]. This is the
//! exact primitive `subms-ts-expr` produces, so the join output drops straight
//! into the expression evaluator.
//!
//! # Column collisions
//!
//! Both inputs can carry a column of the same name. The output renames the
//! collision with `_left` / `_right` suffixes (the join keys themselves are
//! emitted once, unsuffixed, from the left). [`TsJoinResult::column_names`]
//! reflects the final names.
//!
//! # This is the equi-join. asof lives elsewhere.
//!
//! This recipe is the EQUALITY join matrix. The time-series asof / as-of-prior
//! join (match on nearest-prior key within a tolerance) is its own recipe,
//! [`subms-ts-asof-join`](https://www.submillisecond.com/cookbook/recipes/subms-ts-asof-join).
//! A `subms-ts-categorical` dictionary can pre-encode string keys to `u32` for
//! cheaper probes; that is a composition, not a dependency here.
//!
//! # Contract
//!
//! Throughput-contracted, NOT per-op sub-ms. A join is a whole-frame operation;
//! the honest number is rows/sec, captured in `perf/{rust,java}.json`. The bench
//! asserts only a generous no-pathological-stall guard, not a tight p99.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries};
//! use subms_ts_join::{hash_join, TsJoinKind};
//!
//! // left: sym -> px ; right: sym -> qty. "MSFT" is left-only, "GOOG" right-only.
//! let mut sym_l = TsSeries::<String>::new();
//! let mut px = TsSeries::<f64>::new();
//! for (i, (sym, p)) in [("AAPL", 10.0), ("MSFT", 20.0)].iter().enumerate() {
//!     sym_l.push(i as i64, sym.to_string()).unwrap();
//!     px.push(i as i64, *p).unwrap();
//! }
//! let left = TsDataFrame::new()
//!     .with_column("sym", TsColumn::Str(sym_l))
//!     .with_column("px", TsColumn::F64(px));
//!
//! let mut sym_r = TsSeries::<String>::new();
//! let mut qty = TsSeries::<f64>::new();
//! for (i, (sym, q)) in [("AAPL", 100.0), ("GOOG", 300.0)].iter().enumerate() {
//!     sym_r.push(i as i64, sym.to_string()).unwrap();
//!     qty.push(i as i64, *q).unwrap();
//! }
//! let right = TsDataFrame::new()
//!     .with_column("sym", TsColumn::Str(sym_r))
//!     .with_column("qty", TsColumn::F64(qty));
//!
//! let out = hash_join(&left, &right, &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
//! assert_eq!(out.nrows(), 1); // only "AAPL" matches
//! ```

mod join;

pub use join::{
    TsJoinError, TsJoinKind, TsJoinResult, cross_join, frame_columns, hash_join, sort_merge_join,
};

// Re-export the result array so a downstream caller doesn't have to also depend
// on subms-ts-expr just to read a join's output cells.
pub use subms_ts_expr::TsArray;

#[cfg(feature = "harness")]
pub mod recipe;
