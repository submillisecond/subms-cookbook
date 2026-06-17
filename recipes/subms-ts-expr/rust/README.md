# subms-ts-expr

A small TYPED expression IR evaluated over a heterogeneous `TsDataFrame`,
producing a typed nullable `TsArray`. The substrate of the
[submillisecond](https://www.submillisecond.com) cookbook's analytical layer -
the lazy planner and the groupby / join / reshape / window recipes build on
this IR.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-expr>
- **Crate:** `subms-ts-expr` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-expr`

## Install

```toml
[dependencies]
subms-ts-expr = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::{eval_scalar, when, TsExpr};

let mut open = TsSeries::<f64>::new();
let mut close = TsSeries::<f64>::new();
for i in 0..4 {
    open.push(i, i as f64).unwrap();
    close.push(i, (i as f64) + if i % 2 == 0 { 1.0 } else { -1.0 }).unwrap();
}
let f = TsDataFrame::new()
    .with_column("open", TsColumn::F64(open))
    .with_column("close", TsColumn::F64(close));

let expr = when(
    TsExpr::col("close").gt(TsExpr::col("open")),
    TsExpr::col("close").sub(TsExpr::col("open")),
    TsExpr::lit_f64(0.0),
)
.mean();
assert_eq!(eval_scalar(&expr, &f).unwrap(), TsValue::F64(0.5));
```

## What it ships

- `TsExpr` - the IR tree: `Col` / `Lit(TsValue)` / `Unary` / `Binary` /
  `Compare` / `When` / `Agg`, with fluent builders (`.add` / `.gt` / `.mean` /
  ...), typed literal builders (`lit_f64` / `lit_i64` / `lit_bool` / `lit_str`),
  and the free `when(cond, then, otherwise)`.
- `eval(&TsExpr, &TsDataFrame) -> Result<TsArray, TsExprError>` - walks the tree
  column-at-a-time over the frame's union-of-timestamps row axis. Enforces the
  type rules (numeric promotion, Bool compares, Agg result types) and returns a
  typed nullable array.
- `TsArray` - the typed nullable result: `F64` / `I64` / `Bool` / `Str`, each a
  dense value buffer plus a validity bitmap; `data_type`, `get`, `valid_count`,
  `fill_null`, `drop_nulls`, typed `as_f64` / `as_i64` / `as_bool` / `as_str`
  views.
- `eval_scalar(&TsExpr, &TsDataFrame) -> Result<TsValue, TsExprError>` - the
  single scalar of a top-level `Agg`; the operators' per-group entry point.

## Status

`0.6.0`. Throughput-contracted (the analytical front), NOT a per-op sub-ms
primitive: a full evaluation of a multi-node pipeline over a 4,096-row frame
lands in hundreds of microseconds at p50, with an allocation-bound tail. Typed
over `F64` / `I64` / `Bool` / `Str` columns; lazy optimisation / predicate
pushdown is the future `subms-ts-lazy` recipe that compiles these exprs.

## Licence

MIT OR Apache-2.0.
