# subms-ts-aggregator

Streaming rolling-window aggregator: push points in time order, read
`min` / `max` / `sum` / `mean` / `count` over the last `window_ns` at O(1)
amortised per push, mergeable across partitions. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-aggregator>
- **Crate:** `subms-ts-aggregator` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-aggregator`

## Install

```toml
[dependencies]
subms-ts-aggregator = "0.6"
```

## Quickstart

```rust
use subms_ts_aggregator::TsWindowedAggregator;

let mut a = TsWindowedAggregator::new(1_000);
a.push(0, 5.0);
a.push(900, 9.0);
assert_eq!(a.max(), Some(9.0));
```

## What it ships

- `TsWindowedAggregator` - push, min / max (monotonic deques), sum / mean /
  count (running total), window iterator, and partition merge.

## Status

`0.6.0`. Sub-ms p99 on push / query / merge at a 1024-point window (push +
query ~200 ns, merge ~85 us). Streaming min/max cross-checked against a
brute-force window.

## Licence

MIT OR Apache-2.0.
