# subms-ts-window

SQL-style window functions partitioned by key over a `TsFrame`, built on the
[`subms-ts-expr`](https://www.submillisecond.com/cookbook/recipes/subms-ts-expr)
IR. Each function partitions the frame's rows by a tuple of key columns, orders
the rows inside each partition, applies a per-partition transform, and scatters
the result back onto the original row axis as a `TsColumn`.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-window>
- **Crate:** `subms-ts-window` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-window`

## Install

```toml
[dependencies]
subms-ts-window = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsFrame, TsFrameMetadata, TsSeries};
use subms_ts_window::{cumsum, lag, over};
use subms_ts_expr::TsExpr;

let mut f = TsFrame::<f64>::new(TsFrameMetadata::new("trades"));
let mut sym = TsSeries::new();
let mut px = TsSeries::new();
for i in 0..6 {
    sym.push(i, if i % 2 == 0 { 1.0 } else { 2.0 }).unwrap();
    px.push(i, i as f64).unwrap();
}
f.add_series("sym", sym);
f.add_series("px", px);

let prev = lag(&f, "px", 1, &["sym"]).unwrap();          // previous px per symbol
let running = cumsum(&f, "px", &["sym"], None).unwrap(); // running px per symbol
let avg = over(&f, &TsExpr::col("px").mean(), &["sym"]).unwrap(); // mean OVER PARTITION
```

## What it ships

- `lag` / `lead` - shift a column within each partition; out-of-range cells
  invalid.
- `row_number` / `rank` / `dense_rank` - within-partition numbering and ranks
  (ties share a rank).
- `cumsum` / `cumprod` / `cummin` / `cummax` - running reductions per
  partition, in order-by order.
- `over(frame, agg_expr, partition_by)` - evaluate a `TsExpr` aggregation over
  each partition's sub-frame and broadcast the scalar back to every row.

Every function returns a `TsColumn` aligned to the frame's row axis, so window
outputs compose straight back into `subms-ts-expr` evaluation.

## Status

`0.6.0`. THROUGHPUT-contracted (the analytical front), NOT a per-op sub-ms
primitive: a full window pass over a partitioned 4,096-row frame lands in tens
of microseconds typical, with an allocation-bound tail. `f64` columns only.
Frame bounds beyond the whole partition (SQL `ROWS BETWEEN` / `RANGE BETWEEN`)
and parallel execution are out of scope; see the recipe page.

## Licence

MIT OR Apache-2.0.
