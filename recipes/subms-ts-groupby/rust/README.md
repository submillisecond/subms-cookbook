# subms-ts-groupby

Typed, multi-key group-by with multi-aggregation over a heterogeneous
`TsDataFrame`, the defining Polars / DuckDB operation. Built on the
[submillisecond](https://www.submillisecond.com) cookbook's `subms-ts-expr` IR:
the keys partition the frame's rows by the tuple of TYPED cell values (a `Str`
symbol, an `I64` id, an `F64`, a `Bool`), and each aggregation is an ordinary
`TsExpr` reduced per group.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-groupby>
- **Crate:** `subms-ts-groupby` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-groupby`

## Install

```toml
[dependencies]
subms-ts-groupby = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::TsExpr;
use subms_ts_groupby::group_by;

let mut symbol = TsSeries::<String>::new();
let mut size = TsSeries::<f64>::new();
for (i, (sym, sz)) in [("AAPL", 10.0), ("MSFT", 5.0), ("AAPL", 7.0), ("MSFT", 3.0)]
    .into_iter()
    .enumerate()
{
    symbol.push(i as i64, sym.to_string()).unwrap();
    size.push(i as i64, sz).unwrap();
}
let f = TsDataFrame::new()
    .with_column("symbol", TsColumn::Str(symbol))
    .with_column("size", TsColumn::F64(size));

let result = group_by(&f, &["symbol"])
    .unwrap()
    .agg(&[("total_size", TsExpr::col("size").sum())])
    .unwrap();

assert_eq!(result.value("total_size", 0), Some(TsValue::F64(17.0))); // AAPL
assert_eq!(result.value("total_size", 1), Some(TsValue::F64(8.0)));  // MSFT
```

## What it ships

- `group_by(&frame, &[keys]) -> TsGroupBy` - single-pass hash partition of the
  frame's aligned rows by the tuple of typed key cells. Null-key rows are dropped.
- `TsGroupBy::agg(&[(out_name, TsExpr)]) -> TsGroupResult` - reduce each
  top-level `Agg` expression per group; a positional named-`TsArray` table, one
  row per group, key columns first then aggregations, key-sorted.
- `value_counts(&frame, column)` - key column + `count`, descending count.
- `unique(&frame, &[columns])` - distinct key tuples, key-sorted.
- `top_k(&frame, column, k)` - the `k` rows largest by a numeric column, as a
  reordered frame.
- `sort_by(&frame, &[columns], ascending)` - multi-key lexicographic sort,
  nulls-last, as a reordered frame.

## Status

`0.6.0`. THROUGHPUT-contracted (the analytical front), NOT a per-op sub-ms
primitive: a group-by-aggregate over a few thousand rows lands in low-single-digit
milliseconds. Keys on any typed column. Single-threaded (no Rayon), in-memory
(no spill). Lazy / pushdown optimisation is the future `subms-ts-lazy` recipe.

## Licence

MIT OR Apache-2.0.
