# subms-ts-fill

Gap fill for time series - linear interpolation, last-observation-carried-
forward (LOCF), and zero fill on a regular step, preserving original points.
Part of the [submillisecond](https://www.submillisecond.com) cookbook
`timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-fill>
- **Crate:** `subms-ts-fill` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-fill`

## Install

```toml
[dependencies]
subms-ts-fill = "0.6"
```

## Quickstart

```rust
use subms_ts::TsSeries;
use subms_ts_fill::fill_linear;

let mut s = TsSeries::<f64>::new();
s.push(0, 0.0).unwrap();
s.push(40, 4.0).unwrap();
let filled = fill_linear(&s, 10); // inserts 1.0,2.0,3.0 at 10,20,30
assert_eq!(filled.len(), 5);
```

## What it ships

- `fill_linear` / `fill_locf` / `fill_zero(series, step_ns)` -> a new
  `TsSeries` with gaps wider than `step_ns` filled per policy.

## Status

`0.6.0`. Sub-ms p99 per full fill of a 1024-point series (~126 us linear,
~60 us LOCF). Each policy verified exactly.

## Licence

MIT OR Apache-2.0.
