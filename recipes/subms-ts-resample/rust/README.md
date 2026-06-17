# subms-ts-resample

Resample an irregular time series onto a regular grid (mean / last / first /
sum / count / min / max). Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-resample>
- **Crate:** `subms-ts-resample` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-resample`

## Install

```toml
[dependencies]
subms-ts-resample = "0.6"
```

## Quickstart

```rust
use subms_ts::TsSeries;
use subms_ts_resample::{resample_to_grid, TsResampleMode};

let mut s = TsSeries::<f64>::new();
s.push(0, 1.0).unwrap();
s.push(3, 3.0).unwrap();
let g = resample_to_grid(&s, 10, TsResampleMode::Mean); // bucket [0,10) -> mean 2.0
```

## What it ships

- `resample_to_grid(series, period_ns, TsResampleMode)` -> a regular-grid
  `TsSeries`; modes Mean / Last / First / Sum / Count / Min / Max.

## Status

`0.6.0`. Sub-ms p99 per full resample of a 1024-point series (~8 us).
Absolute-time bucket alignment; every mode verified exactly.

## Licence

MIT OR Apache-2.0.
