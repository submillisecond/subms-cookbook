# subms-ts-downsampler

Tiered downsampling pipeline: push raw points once, each tier (1s / 1m / 1h /
...) rolls them into fixed-width buckets emitting `(count, sum, min, max,
last)`. Part of the [submillisecond](https://www.submillisecond.com) cookbook
`timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-downsampler>
- **Crate:** `subms-ts-downsampler` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-downsampler`

## Install

```toml
[dependencies]
subms-ts-downsampler = "0.6"
```

## Quickstart

```rust
use subms_ts_downsampler::TsDownsampler;

const S: i64 = 1_000_000_000;
let mut d = TsDownsampler::new(&[S, 60 * S]);
for i in 0..600 { d.push(i * 100_000_000, i as f64); }
d.flush();
assert_eq!(d.tier(1).len(), 1); // one 1-minute bucket
```

## What it ships

- `TsDownsampler` - new(tier_durations_ns), push (feeds all tiers), flush,
  tier(level) closed-bucket mean series, bucket_stats(level, ts) full rollup.
- `TsBucketStats` (count / sum / min / max / last + mean()).

## Status

`0.6.0`. Sub-ms p99 on push + bucket_stats (~200 ns each) for a 3-tier
pipeline.

## Licence

MIT OR Apache-2.0.
