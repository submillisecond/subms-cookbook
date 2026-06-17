# subms-tdigest

Streaming quantile sketch (t-digest, merging variant + k1 scale): constant
memory, mergeable, relative-error bounded, tightest at the tails. Part of the
[submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-tdigest>
- **Crate:** `subms-tdigest` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-tdigest`

## Install

```toml
[dependencies]
subms-tdigest = "0.6"
```

## Quickstart

```rust
use subms_tdigest::TsTDigest;

let mut d = TsTDigest::new(100.0);
for i in 0..1_000_000 { d.add((i % 1000) as f64); }
let p99 = d.quantile(0.99);
let bytes = d.serialize(); // ~925 bytes regardless of input size
```

## What it ships

- `TsTDigest` - new(compression), add / add_weighted, quantile(q), cdf(value),
  merge, serialize / deserialize (versioned, byte-equivalent across languages).

## Status

`0.6.0`. Sub-ms p99 on add (~200 ns), quantile (~300 ns), merge (~5 us);
constant ~925-byte footprint. Quantiles cross-validated against the exact
sorted array and Dunning's reference behaviour.

## Licence

MIT OR Apache-2.0.
