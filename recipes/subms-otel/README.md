# subms-otel

**OpenTelemetry bridge for the subms perf harness.** Register one observer
on a `SubMsPerfHarness` and every recorded sample plus the post-bench
summary lands in your existing OTel pipeline - histograms named
`subms.latency` (unit `s`), kind-aware bucket boundaries, the full
`subms.*` attribute set, exemplars tagged with the active W3C `trace_id`,
plus spans for the bench loop itself.

[![crates.io](https://img.shields.io/crates/v/subms-otel.svg?logo=rust&style=flat-square)](https://crates.io/crates/subms-otel)
[![maven central](https://img.shields.io/maven-central/v/com.submillisecond/subms-otel.svg?logo=apache-maven&style=flat-square)](https://central.sonatype.com/artifact/com.submillisecond/subms-otel)
[![ci](https://github.com/submillisecond/subms-otel/actions/workflows/ci.yml/badge.svg)](https://github.com/submillisecond/subms-otel/actions/workflows/ci.yml)
[![license](https://img.shields.io/badge/license-MIT_OR_Apache--2.0-blue.svg?style=flat-square)](#license)
[![docs.rs](https://img.shields.io/badge/docs.rs-subms--otel-blue?style=flat-square&logo=rust)](https://docs.rs/subms-otel)

> Eight features grouped as five capability flags plus three exporter
> flags. Pick what you ship; the binary only links what you pick.

The deep treatment lives at
<https://submillisecond.com/cookbook/recipes/subms-otel>. This README is
the elevator pitch + the feature taxonomy.

## What ships

| Flag                  | Capability                                                                                                                                       |
|-----------------------|--------------------------------------------------------------------------------------------------------------------------------------------------|
| `bridge` (default)    | Post-hoc `exportSummary` / `exportTimer`, `CompositeObserver` fan-out, resource semconv, counter + gauge surface, reference-divergence counter.  |
| `observer`            | Live `OtelObserver` (sync) + `OtelObserverAsync` (lock-free SPSC ring, drop-oldest under back-pressure).                                         |
| `exemplars`           | Per-bucket reservoir of the slowest K sampled ops (default K=5), each tagged with the W3C `trace_id` when tracing is on.                         |
| `tracing`             | Span per `stage.record` + W3C TraceContext parent inheritance, plus the post-hoc `exportTimer` chain.                                            |
| `autoconfig`          | Env-driven one-line SDK wiring + `ServiceLoader` / `inventory` auto-register: drop the jar in, the observer attaches itself.                     |
| `exporter-otlp`       | Thin wiring helper for the OTLP HTTP + gRPC exporters.                                                                                           |
| `exporter-prometheus` | Thin wiring helper for the Prometheus exporter + scrape endpoint registration.                                                                   |
| `exporter-stdout`     | Thin wiring helper for the stdout exporter (dev / CI default).                                                                                   |

## Install

### Rust

```toml
[dependencies]
# Defaults to just the post-hoc bridge.
subms-otel = "0.8"

# Or the recommended production kit:
subms-otel = { version = "0.6", features = ["observer", "exemplars", "tracing", "autoconfig", "exporter-otlp"] }
```

### Java (Maven)

```xml
<dependency>
    <groupId>com.submillisecond.recipes</groupId>
    <artifactId>subms-otel</artifactId>
    <version>0.8.1</version>
</dependency>
```

JDK 21 baseline. Pulls `io.opentelemetry:opentelemetry-api` at compile
scope. OTLP / Prometheus / stdout exporter deps are declared
`<optional>true</optional>`; add them to your own pom when you want them.

## Quickstart

```rust
use std::sync::Arc;
use subms::SubMsPerfHarness;
use subms_otel::{auto_configure, OtelObserverAsync};

let providers = auto_configure();
let observer  = OtelObserverAsync::new(providers.meter());
let h = SubMsPerfHarness::new("bloom-filter", "rust")
    .with_observer(Arc::new(observer));
// ... run any cookbook recipe; samples, exemplars, spans land in OTel ...
let summary = subms::summarize(&h);
providers.shutdown();
```

```java
try (var providers = SubMsOtelAutoConfig.autoConfigure();
     var observer  = new OtelObserverAsync(providers.meter())) {
    SubMsPerfHarness h = new SubMsPerfHarness("bloom-filter", "java")
        .withObserver(observer);
    // ... run any cookbook recipe ...
    SubMsBench.summarize(h);
}
```

## Dashboards + alerts

Pre-built Grafana boards + Prometheus alerts ship in
[`dashboards/`](dashboards/). Import the JSON in Grafana, drop the YAML
into your Prometheus rules path. Full instructions in
[`dashboards/README.md`](dashboards/README.md).

## License

Dual-licensed under either of:

- [MIT License](LICENSE-MIT)
- [Apache License 2.0](LICENSE-APACHE)

at your option.

## See also

- Recipe page: <https://submillisecond.com/cookbook/recipes/subms-otel>
- Harness: <https://github.com/submillisecond/subms>
- Cookbook: <https://submillisecond.com/cookbook>
