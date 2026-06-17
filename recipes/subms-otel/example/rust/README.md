# subms-primer-otel (Rust)

The runnable companion to the `subms-otel` primer. Shows the headline
pattern: register one observer against a `SubMsPerfHarness` and every
recorded sample (plus the post-bench summary) lands in OpenTelemetry,
without the workload itself ever pulling in the OTEL dep tree.

This crate is not published. It exists to be read, run, and copied from
when you start your own perf-harness-to-OTEL wiring.

## What it shows

- A tiny `TinyMap<u32, u32>` (open-addressed, linear probing) - the
  toy data structure standing in for "any cookbook recipe".
- `run_workload` - builds a `SubMsPerfHarness` annotated with the
  standard `subms.recipe.slug` / `subms.recipe.category` / `host` /
  `hardware_tier` / `crate_version` meta keys, then drives `put` /
  `get_hit` / `get_miss` stages, each tagged `SubMsStageKind::HotPath`.
- `examples/otel_main.rs` - the heart of the primer. Runs the workload
  twice:
  - Once under the synchronous `OtelObserver`: every recorded sample
    hits the meter on the calling thread.
  - Once under the asynchronous `OtelObserverAsync`: samples land in a
    65k-cap channel and the background worker drains every 100ms;
    `flush()` forces a final drain before the provider shutdown.
  Both observers receive the same `on_summarize` callback after
  `summarize`, which re-emits the percentile set under the fuller
  attribute set drawn from inputs + meta.

The workload contains zero OTEL code. The observer does it all.

## Run the example

```sh
cargo run --release --example otel_main
```

Prints the harness summary for each run, then a rollup of the OTEL
signal captured by the in-memory exporter (a stand-in for a stdout /
OTLP / Prometheus exporter - the observer wiring is the same either
way). The `dropped samples` line under the async run reports
back-pressure events; with the default 65k-cap channel and a 5k-op
workload it should always read zero.

## Run the tests

```sh
cargo test --release
```

Covers TinyMap correctness, workload-stage shape, and end-to-end OTEL
attribute round-tripping under both the sync and async observers.

## Layout

- `src/lib.rs` - `TinyMap`, `WorkloadParams`, `run_workload`,
  `standard_harness`, plus re-exports of the observer types so the
  example and tests can import from one place.
- `examples/otel_main.rs` - the end-to-end driver.
- `tests/lib_tests.rs` - TinyMap + workload + observer wiring tests.

## License

MIT OR Apache-2.0.
