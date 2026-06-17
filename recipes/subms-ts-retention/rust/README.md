# subms-ts-retention

Retention policies that prune a `TsSeries` in place by age, point count, or
approximate byte budget. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-retention>
- **Crate:** `subms-ts-retention` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-retention`

## Install

```toml
[dependencies]
subms-ts-retention = "0.6"
```

## Quickstart

```rust
use subms_ts::TsSeries;
use subms_ts_retention::TsRetentionPolicy;

let mut s = TsSeries::<f64>::new();
for i in 0..1000 { s.push(i, i as f64).unwrap(); }
let removed = TsRetentionPolicy::new().max_points(100).apply(&mut s);
assert_eq!(removed, 900);
assert_eq!(s.len(), 100); // newest 100 kept
```

## What it ships

- `TsRetentionPolicy` - chained `max_age_ns` / `max_points` / `max_bytes`
  limits, `apply(&mut series)` returns the count removed, `apply_all(iter)`
  folds over a collection. Age is applied first, then the tighter of the count
  / byte caps. Built on the series' own `truncate_before` + `retain`, so it
  adds no storage.

## Status

`0.6.0`. Pure compute over the existing delete surface; sub-ms p99 to prune a
4096-point series. Byte budget is approximate at `BYTES_PER_POINT` (16) and
count caps round at ts-tie boundaries the same way `truncate_before` does.

## Licence

MIT OR Apache-2.0.
