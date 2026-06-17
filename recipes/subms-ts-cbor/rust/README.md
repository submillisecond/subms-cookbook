# subms-ts-cbor

Zero-dep CBOR codec for `TsSeries<f64>` - a compact, deterministic columnar
binary form. Implements the `TsCodec` substrate from
[`subms-ts`](https://www.submillisecond.com/cookbook/recipes/subms-ts). Part of
the [submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-cbor>
- **Crate:** `subms-ts-cbor` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-cbor`

## Install

```toml
[dependencies]
subms-ts-cbor = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsCodec, TsSeries};
use subms_ts_cbor::TsCborCodec;

let mut s = TsSeries::<f64>::new();
s.push(1, 1.5).unwrap();
s.push(2, 2.5).unwrap();
let bytes = TsCborCodec::new().encode(&s);
let back = TsCborCodec::new().decode(&bytes).unwrap();
assert_eq!(back.len(), 2);
```

## What it ships

- `TsCborCodec` - hand-rolled CBOR encode/decode of the columnar series form
  (`{"ts": [..], "v": [..]}`). Canonical layout (fixed key order, minimal int
  heads, definite-length arrays, float64 values), so the bytes are
  byte-equivalent across the Rust + Java ports. `TsCborError` on malformed
  input.

## Status

`0.6.0`. Values round-trip bit-exact. Encode + decode of a 1024-point series
are sub-ms p99. Carries the data columns only; series metadata is not on the
wire (same as the columnar JSON codec).

## Licence

MIT OR Apache-2.0.
