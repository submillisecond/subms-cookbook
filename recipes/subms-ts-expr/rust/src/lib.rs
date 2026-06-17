//! `subms-ts-expr` - a small TYPED expression IR evaluated over a heterogeneous
//! [`TsDataFrame`](subms_ts::TsDataFrame), producing a typed nullable
//! [`TsArray`]. This is the substrate of the cookbook's analytical layer: the
//! lazy planner, groupby, join, reshape, and window recipes all build on this
//! IR, so the tree shape, the type rules, and the result array are the stable
//! contract.
//!
//! A [`TsExpr`] is an immutable tree (`Col` / `Lit` / `Unary` / `Binary` /
//! `Compare` / `When` / `Agg`). [`eval`] walks it column-at-a-time against a
//! frame, producing one [`TsArray`] aligned to the frame's union-of-timestamps
//! row axis. A [`TsArray`] is one variant per element type (`F64` / `I64` /
//! `Bool` / `Str`), each a dense value buffer plus an Arrow-style validity
//! bitmap: a cell is meaningful only where its valid bit is set.
//!
//! The IR is type-erased; the type rules live in [`eval`]. A `Col` takes its
//! column's `TsDataType`; arithmetic promotes `I64`/`F64` to `F64`; comparison
//! yields `Bool`; an `Agg` resolves the result type its reduction defines. A
//! mismatch is a [`TsExprError::TypeMismatch`], not a silent coercion.
//!
//! The validity model is for DERIVED nulls - a `Col` over a row where the column
//! has a gap, a divide-by-zero, a null propagated through a binary op. It is
//! distinct from `TsSeries`' no-null-on-ingest invariant: a series never stores
//! a null, but aligning several series onto a shared row axis (what a frame
//! does) legitimately produces missing cells.
//!
//! This recipe is throughput-contracted, not per-op sub-ms: it is the
//! analytical front, not the tick loop. The bench measures whole-frame eval
//! throughput; the assertion is a generous "does not stall pathologically"
//! bound, not a tight per-op p99.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
//! use subms_ts_expr::{eval_scalar, when, TsExpr};
//!
//! let mut open = TsSeries::<f64>::new();
//! let mut close = TsSeries::<f64>::new();
//! for i in 0..4 {
//!     open.push(i, i as f64).unwrap();
//!     close.push(i, (i as f64) + (if i % 2 == 0 { 1.0 } else { -1.0 })).unwrap();
//! }
//! let f = TsDataFrame::new()
//!     .with_column("open", TsColumn::F64(open))
//!     .with_column("close", TsColumn::F64(close));
//!
//! // mean up-move: where close > open, close - open, else 0.
//! let expr = when(
//!     TsExpr::col("close").gt(TsExpr::col("open")),
//!     TsExpr::col("close").sub(TsExpr::col("open")),
//!     TsExpr::lit_f64(0.0),
//! )
//! .mean();
//! assert_eq!(eval_scalar(&expr, &f).unwrap(), TsValue::F64(0.5));
//! ```

mod array;
mod eval;
mod expr;

pub use array::TsArray;
pub use eval::{TsExprError, eval, eval_scalar};
pub use expr::{TsAggOp, TsBinaryOp, TsCmpOp, TsExpr, TsUnaryOp, when};

#[cfg(feature = "harness")]
pub mod recipe;
