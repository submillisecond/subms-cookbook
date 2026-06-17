# subms-ts-adapter-mongodb

A MongoDB adapter for the submillisecond cookbook timeseries arc. It maps a
`TsSeries` / `TsCollection` to the canonical time-series document shape behind an
injectable store seam, using the `bson` format library for byte-equivalent
interop. The live driver is opt-in (a Cargo feature / optional Maven dependency);
the default build pulls only the BSON library.

## Mapping

- Each series is one `ts_<sid>` collection of point documents `{ _id: { sid, ts }, v }`.
- The compound `(_id.sid, _id.ts)` index makes a per-series range scan a single B-tree walk.
- Series identity (name, tags, schema) lives in a sidecar `ts_meta` document keyed by series id.
- One numeric field `v` per point, matching the `TsSeries<f64>` shape.

## Quickstart (Rust)

```rust
use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_mongodb::{InMemoryMongoStore, TsMongoAdapter};

let mut s = TsSeries::<f64>::new();
s.push(1_780_000_000_000_000_000, 0.42)?;
let s = s.with_metadata(TsSeriesMetadata::new(7, "cpu").with_tag("host", "edge-01"));

let adapter = TsMongoAdapter::with_store(InMemoryMongoStore::new());
adapter.write_series(&s)?;
let back = adapter.read_series(7)?;
```

## Quickstart (Java)

```java
TsSeriesD s = new TsSeriesD();
s.push(1_780_000_000_000_000_000L, 0.42);
s = s.withMetadata(new TsSeriesMetadata(7, "cpu").withTag("host", "edge-01"));

TsMongoAdapter<InMemoryMongoStore> adapter = new TsMongoAdapter<>(new InMemoryMongoStore());
adapter.writeSeries(s);
TsSeriesD back = adapter.readSeries(7);
```

## Testing without a server

The store is injectable. Tests use `InMemoryMongoStore`, which records inserts
and replays them, so the BSON mapping and the read path are fully unit-tested
with no network. A cross-language hex fixture pins the exact BSON bytes of a
canonical point document in both suites.

## The live driver

Opt in with the `driver` Cargo feature (Rust) or by adding the
`mongodb-driver-sync` dependency (Java). `DriverMongoStore` then backs the same
seam against a real deployment. It is the one class excluded from coverage - the
live network boundary.

## Status

`category: adapter` (required `bson` format dependency). Per-op sub-ms: encode /
decode of one point document asserts p99 < 1 ms. Bulk-batch throughput and the
network round trip are reported, not claimed. Published as `subms-ts-adapter-mongodb` on
crates.io and `com.submillisecond.recipes:subms-ts-adapter-mongodb` on Maven Central.

## Licence

MIT OR Apache-2.0.
