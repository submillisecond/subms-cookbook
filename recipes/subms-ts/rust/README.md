# subms-ts

The generic time-series core of the [submillisecond](https://www.submillisecond.com)
cookbook `timeseries` arc. `TsSeries<T>` over a three-temperature chunked
store: a mutable SoA head chunk that absorbs `push` in tens of nanoseconds,
sealing into immutable warm chunks, with a Gorilla-compressed cold tier that
plugs in behind the same range view.

- **Recipe page:** <https://www.submillisecond.com/cookbook/recipes/subms-ts>
- **Crate:** `subms-ts` on crates.io
- **Maven:** `com.submillisecond.recipes:subms-ts`

## Install

```toml
[dependencies]
subms-ts = "0.6"
```

Optional features: `datetime` (chrono conversions over the i64-nanos core),
`harness` (the `subms` bench wiring).

## Quickstart

```rust
use subms_ts::TsSeries;

let mut s = TsSeries::<f64>::new();
s.push(1_000, 10.0).unwrap();
s.push(2_000, 12.5).unwrap();
assert_eq!(s.nearest_before(1_500).map(|p| p.value), Some(10.0));
assert_eq!(s.max(), Some(12.5));
```

## What it ships

- `TsPoint<T>` / `TsSeries<T>` - chunked storage, time queries (`nearest`,
  `range`, as-of), numeric aggregates (gated on `TsNumeric`), full delete
  surface, no-null + monotonic ingest.
- Value types `Ohlc` / `Curve` / `Surface` + schemaless `TsValue`.
- `TsSeriesMetadata` (schema / tags / normalised attrs / deps / format).
- `TsCodec` trait + `TsJsonCodec` (epoch nanos / millis / ISO-8601).
- `TsCollection<T>` (by-id registry) + `TsPanel<T>` (homogeneous named slots +
  aligned view) + `TsDataFrame` (heterogeneous, type-erased `TsColumn` columns).

## Status

`0.6.0`. Sub-ms p99 contract on `push` / `nearest` / `range_min` /
`range_sum` at 50k points (measured under 1 us p99). SIMD scans + the Arrow
zero-copy + Gorilla cold tier land in sibling recipes.

## Licence

MIT OR Apache-2.0.
