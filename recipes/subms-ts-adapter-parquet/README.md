# subms-ts-adapter-parquet

An Apache Parquet adapter for the submillisecond cookbook timeseries arc. It
persists a `TsSeries` / `TsCollection` to a self-describing Parquet file, readable
by Spark / DuckDB / pandas / Polars, and reads it back.

## Mapping

- A series is a Parquet file with columns `ts` (int64), `v` (double); identity (id, name, tags) rides in the file key-value metadata.
- A collection is the long-format file `sid`, `ts`, `v`.
- The file is standard Parquet (starts and ends with `PAR1`), so any Parquet reader ingests it.

## Quickstart (Rust)

```rust
use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_parquet::{series_to_parquet, parquet_to_series};

let mut s = TsSeries::<f64>::new();
s.push(1_780_000_000_000_000_000, 0.42)?;
let s = s.with_metadata(TsSeriesMetadata::new(1, "cpu").with_tag("host", "edge-01"));

let bytes = series_to_parquet(&s)?;
let back = parquet_to_series(&bytes)?;
```

## Quickstart (Java)

```java
TsSeriesD s = new TsSeriesD();
s.push(1_780_000_000_000_000_000L, 0.42);
s = s.withMetadata(new TsSeriesMetadata(1, "cpu").withTag("host", "edge-01"));

byte[] bytes = ParquetConvert.seriesToParquet(s);
TsSeriesD back = ParquetConvert.parquetToSeries(bytes);
```

## Port asymmetry

The Rust port composes on `subms-ts-adapter-arrow` (parquet-rs has an Arrow bridge); the
Java port maps directly to a Parquet `MessageType` via parquet-mr's Group API
(parquet-mr has no clean Arrow bridge). The on-disk Parquet file both produce is
the shared interop surface - a file written by one port reads in any Parquet
consumer, including the other.

## Status

`category: adapter` (required `parquet` / parquet-mr dependency). Per-op sub-ms:
encode / decode of a modest series asserts p99 < 1 ms; large-file throughput is
reported, not claimed. Published as `subms-ts-adapter-parquet` on crates.io and
`com.submillisecond.recipes:subms-ts-adapter-parquet` on Maven Central.

## Licence

MIT OR Apache-2.0.
