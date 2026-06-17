# subms-ts-adapter-influxdb

A zero-dependency InfluxDB v2 adapter for the submillisecond cookbook timeseries
arc. It maps a `TsSeries` / `TsCollection` to InfluxDB by hand-rolling the
line-protocol encoder (writes) and the annotated-CSV decoder (reads) over an
injectable HTTP transport - no third-party client library.

## Mapping

- Each series is one measurement (its metadata `name`, or an explicit override).
- Series metadata tags become the Influx tag set, emitted in key order.
- The point value is a single field `v`; timestamps are nanoseconds (`precision=ns`).
- A decoded series is named by its Influx series key (`measurement,k=v,...`),
  unique per (measurement, tag-set), and remains queryable by tag.

## Quickstart (Rust)

```rust
use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_influxdb::TsInfluxAdapter;

let mut s = TsSeries::<f64>::new();
s.push(1_780_000_000_000_000_000, 0.42)?;
let s = s.with_metadata(TsSeriesMetadata::new(1, "cpu").with_tag("host", "edge-01"));

let influx = TsInfluxAdapter::connect("http://localhost:8086", "my-token", "my-org", "metrics")?;
influx.write_series(&s, "")?;                     // line-protocol POST
let back = influx.query_flux("from(bucket:\"metrics\") |> range(start:-1h)")?;
```

## Quickstart (Java)

```java
TsSeriesD s = new TsSeriesD();
s.push(1_780_000_000_000_000_000L, 0.42);
s = s.withMetadata(new TsSeriesMetadata(1, "cpu").withTag("host", "edge-01"));

TsInfluxAdapter influx =
    TsInfluxAdapter.connect("http://localhost:8086", "my-token", "my-org", "metrics");
influx.writeSeries(s, "");
TsCollection<Double> back =
    influx.queryFlux("from(bucket:\"metrics\") |> range(start:-1h)");
```

## Testing without a server

The HTTP transport is injectable. Tests pass a `CaptureTransport` that records
requests and replays canned responses, so the line-protocol shaping and the CSV
decoding are fully unit-tested with no network.

## Status

Zero-dep. `category: timeseries`. Throughput-contracted (the network round trip
is reported, not claimed). Published as `subms-ts-adapter-influxdb` on crates.io and
`com.submillisecond.recipes:subms-ts-adapter-influxdb` on Maven Central.

## Licence

MIT OR Apache-2.0.
