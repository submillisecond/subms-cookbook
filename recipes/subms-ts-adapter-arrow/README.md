# subms-ts-adapter-arrow

An Apache Arrow adapter for the submillisecond cookbook timeseries arc. It maps a
`TsSeries` / `TsCollection` to columnar Arrow batches and Arrow IPC streams, so a
series produced anywhere in the arc hands off to Polars / DuckDB / pandas with no
translation layer.

## Mapping

- A series becomes a two-column batch (`ts: Int64`, `v: Float64`); identity (id, name, tags) rides in schema metadata.
- A collection becomes the tidy long-format batch (`sid`, `ts`, `v`) that columnar engines group on.
- Both round-trip through the Arrow IPC stream format.

## Quickstart (Rust)

```rust
use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_arrow::{series_to_batch, write_ipc, read_ipc, batch_to_series};

let mut s = TsSeries::<f64>::new();
s.push(1_780_000_000_000_000_000, 0.42)?;
let s = s.with_metadata(TsSeriesMetadata::new(1, "cpu").with_tag("host", "edge-01"));

let ipc = write_ipc(&series_to_batch(&s)?)?;
let back = batch_to_series(&read_ipc(&ipc)?)?;
```

## Quickstart (Java)

```java
// run with: --add-opens=java.base/java.nio=ALL-UNNAMED
try (RootAllocator alloc = new RootAllocator()) {
    TsSeriesD s = new TsSeriesD();
    s.push(1_780_000_000_000_000_000L, 0.42);
    s = s.withMetadata(new TsSeriesMetadata(1, "cpu").withTag("host", "edge-01"));

    byte[] ipc = ArrowConvert.seriesToIpc(s, alloc);
    TsSeriesD back = ArrowConvert.ipcToSeries(ipc, alloc);
}
```

## Cross-language interop

The IPC stream is Arrow-spec-compliant and cross-readable: the Java test suite
decodes a stream written by the Rust port and asserts its values + metadata.
Byte-identical IPC across the two Arrow implementations is NOT claimed (the format
carries implementation-defined padding + metadata ordering); cross-readability is.

## Status

`category: adapter` (required `arrow` dependency). Per-op sub-ms: converting a
1,024-point series to / from a batch (steady-state) asserts p99 < 1 ms. Bulk IPC
framing is reported, not claimed. Published as `subms-ts-adapter-arrow` on crates.io and
`com.submillisecond.recipes:subms-ts-adapter-arrow` on Maven Central.

## Licence

MIT OR Apache-2.0.
