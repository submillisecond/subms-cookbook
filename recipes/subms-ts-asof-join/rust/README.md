# subms-ts-asof-join

As-of joins (backward / forward / nearest-within-tolerance) over two time
series via a single merge-walk. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-asof-join>
- **Crate:** `subms-ts-asof-join` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-asof-join`

## Install

```toml
[dependencies]
subms-ts-asof-join = "0.6"
```

## Quickstart

```rust
use subms_ts::TsSeries;
use subms_ts_asof_join::asof_join_backward;

let mut trades = TsSeries::<f64>::new();
trades.push(10, 100.0).unwrap();
let mut quotes = TsSeries::<f64>::new();
quotes.push(5, 99.5).unwrap();
let rows = asof_join_backward(&trades, &quotes);
assert_eq!(rows[0].right.map(|p| p.value), Some(99.5));
```

## What it ships

- `asof_join_backward` / `asof_join_forward` (O(n+m) merge-walk) +
  `asof_join_nearest(tolerance_ns)`; `TsMatch { left, right: Option }`.

## Status

`0.6.0`. Sub-ms p99 per full join of two 1024-point series (~22 us backward,
~28 us nearest). Backward cross-checked against brute force.

## Licence

MIT OR Apache-2.0.
