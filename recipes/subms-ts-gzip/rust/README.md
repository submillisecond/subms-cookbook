# subms-ts-gzip

Zero-dep gzip codec that wraps any inner `TsCodec` - hand-rolled DEFLATE/INFLATE
(RFC 1951) plus the gzip container (RFC 1952). No `flate2`. Output is a real
gzip stream (`gunzip`-able); decode reads arbitrary gzip/zlib output including
dynamic-Huffman blocks. Implements the `TsCodec` substrate from
[`subms-ts`](https://www.submillisecond.com/cookbook/recipes/subms-ts). Part of
the [submillisecond](https://www.submillisecond.com) cookbook `timeseries` arc.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-gzip>
- **Crate:** `subms-ts-gzip` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-gzip`

## Install

```toml
[dependencies]
subms-ts-gzip = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsCodec, TsJsonCodec, TsSeries};
use subms_ts_gzip::TsGzipCodec;

let mut s = TsSeries::<f64>::new();
s.push(1, 1.5).unwrap();
s.push(2, 2.5).unwrap();

let codec = TsGzipCodec::new(TsJsonCodec::new(), 6);
let bytes = codec.encode(&s);          // a real gzip stream
let back = codec.decode(&bytes).unwrap();
assert_eq!(back.len(), 2);
assert_eq!(codec.format(), "gzip+json");
```

## What it ships

- `TsGzipCodec<C, T>` - wraps any inner `TsCodec<T>` (`gzip+json`, `gzip+cbor`).
  `encode` = gzip(inner.encode(series)); `decode` = inner.decode(gunzip(bytes)).
- Hand-rolled DEFLATE encoder (hash-chain LZ77, fixed-Huffman blocks, stored
  fallback), INFLATE decoder (stored + fixed + dynamic Huffman), CRC-32, and
  the gzip header/trailer framing - all zero-dep.
- `gzip` / `gunzip` free functions for raw byte payloads.
- `TsGzipError` (bad magic / method / CRC / size / inflate) wrapped by
  `TsGzipCodecError` alongside the inner codec's own error.

## Interop

`encode` output `gunzip`s with the system `gzip` tool; `decode` reads the system
`gzip` tool's output (which is dynamic-Huffman), verified both directions in the
test suite against `gunzip` / `gzip`.

## Status

`0.6.0`. Values round-trip bit-exact through `gzip+json`. Encode + decode of a
128-point series are sub-ms p99. We emit fixed-Huffman blocks (not dynamic),
so the ratio trails a production zlib by a few percent; we decode all three
block types.

## Licence

MIT OR Apache-2.0.
