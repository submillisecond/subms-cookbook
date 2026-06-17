# subms-otel

OpenTelemetry bridge for the [`subms`](https://crates.io/crates/subms) perf
harness. Plugs into the harness's `SubMsObserver` hook and the post-bench
`SubMsBenchSummary` / `SubMsTimer` types to emit Histogram + Span data without
the harness itself pulling in the OTEL dependency tree.

Pair with the cookbook primer at
<https://submillisecond.com/cookbook/primers/subms-otel> for the walkthrough.

## Install

```toml
[dependencies]
subms = "0.5"
subms-otel = { version = "0.5", features = ["sync"] }
```

Pick the feature that matches the cost you're willing to pay on the bench
hot path:

| Feature      | Surface                                                   |
|--------------|-----------------------------------------------------------|
| `bridge`     | `export_summary`, `export_timer`, `histogram_boundaries`, `CompositeObserver` |
| `sync`       | `OtelObserver` - synchronous `Histogram::record` per sample |
| `async`      | `OtelObserverAsync` - bounded channel + background drain thread |
| `otlp`       | `OtlpBuilder` - opt-in OTLP exporter wiring                |
| `prometheus` | `PrometheusBuilder` - opt-in Prometheus exporter wiring   |

Default features pull in just `bridge`.

## Quickstart - wire `OtelObserver`

```rust
use std::sync::Arc;
use opentelemetry::metrics::MeterProvider;
use opentelemetry_sdk::metrics::SdkMeterProvider;
use subms::{SubMsPerfHarness, summarize};
use subms_otel::OtelObserver;

let provider = SdkMeterProvider::builder().build();
let meter = provider.meter("my-recipe");

let mut h = SubMsPerfHarness::new("my-workload", "rust")
    .with_observer(Arc::new(OtelObserver::new(meter)));

let put = h.stage("put", 1_000);
for _ in 0..1_000 { put.time(|| { /* work */ }); }

// on_summarize re-emits the full attribute set drawn from inputs + meta.
let _ = summarize(&h);
```

## Semantic conventions

Every emission carries this attribute table (keys are stable, consumers
filter on them in dashboards):

| Attribute key            | Source                                            |
|--------------------------|---------------------------------------------------|
| `subms.workload`         | `ctx.workload` / `summary.workload`               |
| `subms.lang`             | `ctx.lang` / `summary.lang`                       |
| `subms.stage`            | `ctx.stage` / stage name                          |
| `subms.stage.kind`       | `ctx.stage_kind.as_str()`                         |
| `subms.recipe.slug`      | `meta["subms.recipe.slug"]`                       |
| `subms.recipe.category`  | `meta["subms.recipe.category"]`                   |
| `subms.workload.feature` | `meta["subms.workload.feature"]`                  |
| `subms.workload.entries` | `inputs["entries"]`                               |
| `subms.workload.seed`    | `inputs["seed"]`                                  |
| `subms.host`             | `meta["host"]`                                    |
| `subms.hardware.tier`    | `meta["hardware_tier"]`                           |
| `subms.crate.version`    | `meta["crate_version"]`                           |

Inputs and meta arrive only via `on_summarize` (the harness's
`ObservationCtx` is intentionally minimal). The sync observer therefore
records hot-path samples with just the first four attributes; the
`on_summarize` hook re-emits the headline percentiles plus downsampled
samples under the fuller set.

## Status

Pre-1.0. The harness JSON contract is stable since `subms` 0.2; the
observer surface is stable since `subms` 0.5.1. The OTLP / Prometheus
helpers are thin starter wrappers - production deployments should wire
their own `MeterProvider` for control over batching, resource attrs, and
transport.

## Licence

Dual licensed under MIT or Apache-2.0.
