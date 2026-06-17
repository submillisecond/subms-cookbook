# subms-ts-anomaly

Streaming rolling-window z-score anomaly detector (windowed mean + std),
O(1) amortised per push. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-anomaly>
- **Crate:** `subms-ts-anomaly` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-anomaly`

## Install

```toml
[dependencies]
subms-ts-anomaly = "0.6"
```

## Quickstart

```rust
use subms_ts_anomaly::TsAnomalyDetector;

let mut d = TsAnomalyDetector::new(1_000, 3.0);
for i in 0..50 { d.push(i, 10.0); }
let hit = d.push(50, 100.0); // spike
assert!(hit.is_some());
```

## What it ships

- `TsAnomalyDetector` - new(window_ns, sigma), push(ts, value) ->
  Option<TsAnomaly{ ts, value, zscore }>, window_count. Scores against the
  trailing window before admitting; flat-baseline jumps still flag.

## Status

`0.6.0`. Sub-ms p99 per push (~200 ns) at a 1024-point window. z-score
cross-checked against manual mean/variance.

## Licence

MIT OR Apache-2.0.
