# subms-gorilla-block

Gorilla-encoded time-series column block (delta-of-delta timestamps +
XOR-delta `f64` values), ~1.3 bytes/point versus 16 raw. The compression
scheme Prometheus / VictoriaMetrics / M3 / InfluxDB run internally, as a
clean embeddable library + the cold tier behind
[`subms-ts`](https://www.submillisecond.com/cookbook/recipes/subms-ts).

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-gorilla-block>
- **Crate:** `subms-gorilla-block` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-gorilla-block`

## Install

```toml
[dependencies]
subms-gorilla-block = "0.6"
```

## Quickstart

```rust
use subms_gorilla_block::TsGorillaBlock;

let mut b = TsGorillaBlock::new();
for i in 0..1_024 { b.append(1_700_000_000 + i, 20.0 + (i / 16) as f64); }
let bytes = b.bytes();
let decoded = TsGorillaBlock::from_bytes(&bytes).unwrap();
assert_eq!(decoded.len(), 1_024);
```

## What it ships

- `TsGorillaBlock` - append / iter / range / merge / bytes / from_bytes /
  decode / stats. Versioned wire format, byte-equivalent across Rust + Java.
- `TsGorillaCodec` - implements `subms_ts::TsCodec<f64>` so a `TsSeries`
  serializes to Gorilla bytes and composes under codec wrappers.
- `TsBlockStats`, `TsBlockError`.

## Status

`0.6.0`. Sub-ms p99 on append / decode / range_scan at a 1024-point block
(append ~200 ns, decode ~33 us, scan ~37 us); 1.32 bytes/point on a stepped
gauge. Lossless, bit-exact f64 round-trip.

## Licence

MIT OR Apache-2.0.
