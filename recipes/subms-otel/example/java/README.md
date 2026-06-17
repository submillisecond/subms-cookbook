# subms-otel - one observer, every cookbook recipe in OTEL

A runnable Java walkthrough of the `subms-otel` bridge. The headline: register
one `OtelObserver` (or `OtelObserverAsync`) on a `SubMsPerfHarness` and every
sample that lands in any stage of any cookbook recipe automatically flows
through to OpenTelemetry as a `subms.latency` histogram, attributed by
`subms.workload`, `subms.stage`, `subms.stage.kind`, and (after summarise) the
full recipe-identity set (`subms.recipe.slug`, `subms.recipe.category`,
`subms.host`, ...).

This primer wires that flow against a toy `TinyMap` workload (`put` /
`get_hit` / `get_miss`), and exports the metric blocks to a small self-
contained `StdoutMetricExporter` so the demo runs offline - no Jaeger /
Prometheus / OTLP collector required.

Features in scope:

- **The observer hook on `SubMsPerfHarness`** - `withObserver(...)` /
  `setObserver(...)`, called for every recorded sample and once on the
  post-bench `summarize` pass.
- **`OtelObserver`** - synchronous bridge. `onRecord` ships the sample
  straight into a shared `DoubleHistogram`; `onSummarize` walks the typed
  summary and emits the full attribute set.
- **`OtelObserverAsync`** - bounded-queue + daemon drainer for hot-path
  callers that cannot pay a synchronous `record` on every op. Drop-oldest
  back-pressure with a `subms.otel.dropped` counter.
- **`SubMsStageKind` annotations** - `Stage.withKind(HOT_PATH)` so observers
  can pick fitting histogram buckets per stage.

```sh
mvn -q package
mvn -q test                                                                      # JUnit 5: TinyMap + Workload + OtelObserver integration

# Smallest possible "observer hook in action" snippet.
mvn -q exec:java -Dexec.mainClass=com.submillisecond.primers.otel.Demo

# Full primer demo: sync observer + async observer, both wired against the same workload.
mvn -q exec:java -Dexec.mainClass=com.submillisecond.primers.otel.OtelMain
```

## What `OtelMain` shows

`OtelMain` runs two passes over the same `TinyMap` workload:

1. Build an SDK `Meter`, wrap it in a synchronous `OtelObserver`, register
   that observer on the harness via `withObserver`, run `put` / `get_hit` /
   `get_miss`, then call `SubMsBench.summarize` to fire the post-bench dump.
   `printSummary` prints the percentile table; `StdoutMetricExporter` prints
   the `subms.latency` metric blocks under the lean ctx attribute set plus
   the fuller summary set.
2. Same shape, but with `OtelObserverAsync` - samples queue into a bounded
   `ArrayBlockingQueue`, a daemon thread drains them off the recorder
   thread, and the call site pays nothing more than an `offer`. `drainNow`
   forces a clean flush at end-of-run for deterministic output.

The point of the primer is not the workload numbers - it is the shape of the
plumbing. The same observer-registration line, dropped into any cookbook
recipe's bench main, gives that recipe full OTEL emission for free.

## Files

- `src/main/java/com/submillisecond/primers/otel/TinyMap.java`
  open-addressing long-keyed map with linear probing. The toy structure the
  workload drives.
- `src/main/java/com/submillisecond/primers/otel/Workload.java`
  declares the standard recipe-identity meta on the harness and runs three
  `HOT_PATH` stages.
- `src/main/java/com/submillisecond/primers/otel/StdoutMetricExporter.java`
  tiny `MetricExporter` that prints every flushed metric to a `PrintStream`.
  Replaces the heavier `opentelemetry-exporter-logging` artefact so the
  primer stays dependency-light.
- `src/main/java/com/submillisecond/primers/otel/OtelMain.java`
  the headline demo - sync observer pass plus async observer pass, both
  printed through the percentile table and the OTEL exporter.
- `src/main/java/com/submillisecond/primers/otel/Demo.java`
  the five-line walkthrough block: build a Meter, register an `OtelObserver`,
  record a handful of samples, summarise.
- `src/test/java/com/submillisecond/primers/otel/TinyMapTest.java`
  JUnit 5 - put / get / overwrite / grow / reject contract.
- `src/test/java/com/submillisecond/primers/otel/WorkloadTest.java`
  JUnit 5 - workload declares the right meta, every stage is `HOT_PATH`,
  observer sees the expected ctx on each record.
- `src/test/java/com/submillisecond/primers/otel/OtelObserverIntegrationTest.java`
  JUnit 5 - end-to-end with `InMemoryMetricExporter`: captured metric is
  named `subms.latency`, carries the expected attribute set, and the
  recorded sample count matches the workload size plus the summary pass.
