# subms-ts-adapter-yaml

Human-readable YAML codec for `TsSeries<f64>` - a clean, diff-friendly columnar
document. Implements the `TsCodec` substrate from
[`subms-ts`](https://www.submillisecond.com/cookbook/recipes/subms-ts). An
`adapter` recipe: it pulls the maintained
[`saphyr`](https://crates.io/crates/saphyr) YAML 1.2 parser, because parsing
arbitrary YAML back into a series is thousands of lines of corner cases. Part of
the [submillisecond](https://www.submillisecond.com) cookbook.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts-adapter-yaml>
- **Crate:** `subms-ts-adapter-yaml` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts-adapter-yaml`

## Install

```toml
[dependencies]
subms-ts-adapter-yaml = "0.6"
```

## Quickstart

```rust
use subms_ts::{TsCodec, TsSeries};
use subms_ts_yaml::TsYamlCodec;

let mut s = TsSeries::<f64>::new();
s.push(1, 1.5).unwrap();
s.push(2, 2.5).unwrap();
let bytes = TsYamlCodec::new().encode(&s);
let back = TsYamlCodec::new().decode(&bytes).unwrap();
assert_eq!(back.len(), 2);
```

The document is a tidy two-column block:

```yaml
subms_ts_series:
  timestamps:
  - 1
  - 2
  values:
  - 1.5
  - 2.5
```

## What it ships

- `TsYamlCodec` - hand-written columnar YAML encode (full control over the clean
  block layout); decode runs through the `saphyr` parser. Timestamps render per
  `TsTimestampStyle` (`EpochNanos` / `EpochMillis` round-trip; `Iso8601` is
  encode-only). `TsYamlError` on malformed input or an ISO-timestamp decode.

## Status

`0.6.0`. Values round-trip bit-exact for the values the shortest-round-trippable
formatting reproduces. Encode of a 128-point series is sub-ms p99 on laptop tier
in both ports; decode is a throughput operation, not a per-op sub-ms primitive -
YAML's text parse is heavier than the binary codecs (the snakeyaml decode in the
Java port is GC-dominated at the tail). Carries the data columns only; series
metadata is not on the wire (same as the JSON and CBOR codecs).

## Licence

MIT OR Apache-2.0.
