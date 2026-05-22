---
title: subms - the cookbook's perf harness
summary: Zero-dependency Rust + Java performance harness used by every cookbook recipe. Records timed samples, computes percentiles, runs scale sweeps, and emits a stable JSON shape that the cookbook web layer renders. Symmetric API on both runtimes.
type: primer
category: tooling
repoPath: subms
order: 1
difficulty: 1
loc: 900
languages: [rust, java]
prereqs:
  - "Familiarity with one of: Rust crates / Maven artifacts"
  - "Roughly what p99 means"
glossary: []
tags:
  - tooling
  - performance
  - testing
perf:
  - { label: "stage.time() overhead",        value: "~50 ns",  note: "one nanoTime / Instant::now + push to Vec / long[]" }
  - { label: "summarise() of 50k samples",   value: "<1 ms",   note: "sort + 4 percentile lookups + mean" }
  - { label: "JSON emit of 500-sample run",  value: "<200 us", note: "stable shape, byte-equivalent across langs" }
references:
  - { title: "Cargo: subms crate",       url: "https://crates.io/crates/subms" }
  - { title: "Maven: com.submillisecond:subms",  url: "https://central.sonatype.com/artifact/com.submillisecond/subms" }
  - { title: "OpenTelemetry Java",       url: "https://opentelemetry.io/docs/languages/java/" }
  - { title: "tracing (Rust)",           url: "https://docs.rs/tracing" }
---

`subms` is the engine under every cookbook recipe. One small library, two byte-equivalent runtimes (Rust and Java), zero runtime dependencies on either side. The cookbook's "submillisecond" promise is whatever this harness measures - so the harness is also what other people consume when they want to bench their own code against the same gate.

This primer walks the public surface: harness, summary, sweep, timer, params, and the p99 assertion.

## Add it as a dependency

### Rust

```toml
[dependencies]
subms = "0.3"
```

If you only need types/utilities (no `Recipe` trait, no `examples/perf_main`), this is all you need - the crate is std-only with no transitive deps.

### Java (Maven)

```xml
<dependency>
    <groupId>com.submillisecond</groupId>
    <artifactId>subms</artifactId>
    <version>0.3.0</version>
</dependency>
```

JDK 21 baseline. No transitive deps.

## SubMsPerfHarness - time named stages

The harness records timed samples per named stage and keeps a small inputs / meta map.

**Rust**

```rust
use subms::SubMsPerfHarness;

let mut h = SubMsPerfHarness::new("my-workload", "rust");
h.input("entries", "50000");
h.add_meta("notes", "warm cache, single thread");

let put = h.stage("put", 50_000);
for _ in 0..50_000 {
    put.time(|| { /* work under test */ });
}

// or explicit ns recording:
let get = h.stage("get", 50_000);
let t0 = std::time::Instant::now();
do_work();
get.record(t0.elapsed().as_nanos() as u64);
```

**Java**

```java
import com.submillisecond.perf.SubMsPerfHarness;

SubMsPerfHarness h = new SubMsPerfHarness("my-workload", "java");
h.input("entries", "50000");
h.meta("notes", "warm cache, single thread");

SubMsPerfHarness.Stage put = h.stage("put", 50_000);
for (int i = 0; i < 50_000; i++) {
    put.time(() -> { /* work under test */ });
}

SubMsPerfHarness.Stage get = h.stage("get", 50_000);
long t0 = System.nanoTime();
doWork();
get.record(System.nanoTime() - t0);
```

`stage.time(...)` adds ~50ns of overhead (one `nanoTime` / `Instant::now` + a push to the per-stage sample buffer). The samples are kept in chronological order, never sampled, never discarded.

## SubMsBenchSummary - typed percentiles

The harness owns raw samples. Analysis lives in a separate typed record so consumers can program against the numbers without parsing JSON.

**Rust**

```rust
use subms::{summarize, summarize_lean, print_summary};

let summary = summarize(&h);           // with downsampled samples_ns
// let summary = summarize_lean(&h);   // count + percentiles only

println!("put p99 = {} ns", summary.stage("put").unwrap().p99_ns);

// or print the standard percentile table:
print_summary(&summary, &mut std::io::stdout())?;
```

**Java**

```java
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;

SubMsBenchSummary summary = SubMsBench.summarize(h);
// SubMsBenchSummary summary = SubMsBench.summarizeLean(h);

System.out.println("put p99 = " + summary.stage("put").orElseThrow().p99Ns() + " ns");

SubMsBench.printSummary(summary, System.out);
```

Output of `print_summary` / `printSummary`:

```text
  stage            p50        p99      p99.9        max       mean
  put            300ns      1.2us    153.9us     3.90ms      1.8us
  get_hit        8.2us     29.5us     85.0us     3.77ms      9.0us
  get_miss       2.4us     29.8us     86.3us    189.1us      6.0us
```

Byte-equivalent across runtimes. The format itself is part of the public contract.

## SubMsBenchSweep - performance under load

A sweep is a list of summaries that share a workload but vary one input - typically `entries` for scale-curve studies, or a feature toggle like `bloom_mode`. The cookbook's on-disk `perf/<lang>.json` is already array-shaped, so a sweep is just the structured view of a multi-run bench.

**Rust**

```rust
use subms::{run_sweep, print_sweep, SubMsBenchParams};

let sweep = run_sweep(
    &MyRecipe,
    &[
        SubMsBenchParams { entries:    50_000, warmup: 5_000, seed: 0 },
        SubMsBenchParams { entries:   500_000, warmup: 5_000, seed: 0 },
        SubMsBenchParams { entries: 5_000_000, warmup: 5_000, seed: 0 },
    ],
    Some("entries"),
);

print_sweep(&sweep, &mut std::io::stdout())?;
```

**Java**

```java
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsBenchSweep;

SubMsBenchSweep sweep = SubMsBench.runSweep(
    new MyRecipe(),
    List.of(
        new SubMsBenchParams(    50_000, 5_000, 0L),
        new SubMsBenchParams(   500_000, 5_000, 0L),
        new SubMsBenchParams(5_000_000, 5_000, 0L)),
    "entries");

SubMsBench.printSweep(sweep, System.out);
```

Output:

```text
stage: put
  entries          count        p50        p99      p99.9        max       mean
  50000            50000      300ns      1.2us     76.3us     1.82ms      1.4us
  500000          500000      350ns      1.5us    100.0us     2.50ms      1.5us
  5000000        5000000      400ns      2.0us    200.0us     4.00ms      1.8us

stage: get_hit
  ...
```

To persist the sweep to disk in the cookbook's canonical JSON shape:

```rust
use subms::sweep_to_json;
let f = std::fs::File::create("perf/rust.json")?;
sweep_to_json(&sweep, &mut std::io::BufWriter::new(f))?;
```

```java
try (var out = new PrintStream(new FileOutputStream("perf/java.json"))) {
    SubMsBench.sweepToJson(sweep, out);
}
```

## SubMsTimer - autostart stopwatch with checkpoints

When you want a single timer through a request flow rather than a recipe-level bench, reach for `SubMsTimer`. Zero deps, sub-microsecond overhead, structured checkpoint list, friendly printer.

**Rust**

```rust
use subms::SubMsTimer;

let mut t = SubMsTimer::new("route-decision");
t.mark("venues-fanned-out");
// venue calls
t.mark("quotes-aggregated");
// pick best
t.stop("response-sent");

t.print(&mut std::io::stdout())?;
// timer "route-decision"  total=3.2us
//   venues-fanned-out    +1.1us       1.1us
//   quotes-aggregated    +800ns       1.9us
//   response-sent *      +1.3us       3.2us

// or feed your own metrics pipeline:
for cp in t.checkpoints() {
    metrics::histogram(cp.label.clone()).record(cp.since_start_ns);
}
```

**Java**

```java
import com.submillisecond.perf.SubMsTimer;

SubMsTimer t = new SubMsTimer("route-decision");
t.mark("venues-fanned-out");
t.mark("quotes-aggregated");
t.stop("response-sent");

t.print(System.out);

for (SubMsTimer.Checkpoint cp : t.checkpoints()) {
    metrics.histogram(cp.label()).record(cp.sinceStartNs());
}
```

**When to graduate to a real span pipeline.** `SubMsTimer` is right for hot-loop, single-process timing where every nanosecond of overhead matters. Once you need distributed traces, sampling, or persistent exporters, switch to [OpenTelemetry Java](https://opentelemetry.io/docs/languages/java/) or the [`tracing`](https://docs.rs/tracing) crate. The output shapes are similar; the production tooling is not.

## SubMsBenchParams - stdin key=value config

Standard params for cookbook benches: `entries`, `warmup`, `seed`. Plus parser helpers for additional knobs.

**Rust**

```rust
use subms::{read_stdin_kv, SubMsBenchParams, parse_usize, parse_bool};

let raw = read_stdin_kv();
let params = SubMsBenchParams::from_map(&raw);
let flush_threshold = parse_usize(&raw, "flush_threshold_bytes", 16_000);
let bloom_on = parse_bool(&raw, "bloom_mode", true);
```

**Java**

```java
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

Map<String, String> raw = SubMsPerfHarness.readStdinKv();
SubMsBenchParams params = SubMsBenchParams.fromMap(raw);
int flushThreshold = SubMsBenchParams.parseInt(raw, "flush_threshold_bytes", 16_000);
boolean bloomOn = SubMsBenchParams.parseBool(raw, "bloom_mode", true);
```

The stdin format is liberal: blank lines and `#` comments allowed.

```text
# Default workload
entries=50000
warmup=5000
seed=0

# Recipe-specific
flush_threshold_bytes=16000
bloom_mode=on
```

## assert_p99_under - the quality gate

Every cookbook recipe ships a `sub_millisecond_bench` test that asserts p99 stays under 1 ms on its claimed workload. If a future change regresses past that line, CI fails. That's the cookbook's contract; nothing else holds the "submillisecond" promise honest.

**Rust**

```rust
use subms::{assert_p99_under, SubMsBenchAssertion};

assert_p99_under(
    &summary,
    &[
        SubMsBenchAssertion { stage: "put",     p99_ns_max: 1_000_000 },
        SubMsBenchAssertion { stage: "get_hit", p99_ns_max: 1_000_000 },
    ],
)
.unwrap();
```

**Java**

```java
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchSummary;

SubMsBench.assertP99Under(summary, List.of(
    new SubMsBench.Assertion("put",     1_000_000L),
    new SubMsBench.Assertion("get_hit", 1_000_000L)));
```

The assertion runs **after** the bench (it sorts the recorded samples and looks up the percentile - no effect on the measured workload itself). It is allowed to fail; the recipe page is allowed to publish a `FAIL` verdict; the cookbook stays honest because the gate is in the test, not in the prose.

For convenience, both runtimes also accept a raw harness directly:

```rust
assert_p99_under(&harness, &[ /* ... */ ])?;   // shortcut: summarises internally
```

```java
SubMsBench.assertP99Under(harness, List.of(/* ... */));
```

## SubMsBenchDiff - regression detection between runs

A diff compares a baseline summary against a candidate and produces a typed delta per stage. Every stage gets per-metric (p50, p99, p99.9, max, mean) baseline vs candidate values, signed delta, and percent change. The `has_regression()` flag is what CI gates key off.

**Rust**

```rust
use subms::{diff_summary, diff_summary_with, print_diff, diff_to_json};

let baseline = summarize_lean(&run_bench(&MyRecipe, &params));
// commit, change code, re-bench...
let candidate = summarize_lean(&run_bench(&MyRecipe, &params));

let diff = diff_summary(&baseline, &candidate);     // 10% default threshold
// or diff_summary_with(&baseline, &candidate, 15.0)

print_diff(&diff, &mut std::io::stdout())?;
diff_to_json(&diff, &mut std::fs::File::create("subms-diff.json")?)?;

if diff.has_regression() {
    eprintln!("Worst regression: {} {:.1}%",
        diff.worst_stage().unwrap().stage,
        diff.worst_stage().unwrap().worst_regression_pct);
}
```

**Java**

```java
import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchDiff;

SubMsBenchDiff diff = SubMsBench.diffSummary(baseline, candidate);
// or diffSummary(baseline, candidate, 15.0)

SubMsBench.printDiff(diff, System.out);
try (var out = new PrintStream(new FileOutputStream("subms-diff.json"))) {
    SubMsBench.diffToJson(diff, out);
}

if (diff.hasRegression()) {
    var worst = diff.worstStage().orElseThrow();
    System.err.println("Worst regression: " + worst.stage()
            + " " + worst.worstRegressionPct() + "%");
}
```

Output:

```text
diff: lsm-tree vs lsm-tree (rust)  threshold=+10.0%
  stage         metric    baseline  candidate      delta     %delta  verdict
  put           p50          300ns      350ns      +50ns     +16.7%  REGRESSED
  put           p99          1.2us      1.3us     +100ns      +8.3%  ok
  put           p99.9      153.9us    170.0us    +16.1us     +10.5%  REGRESSED
  ...
```

JSON output is byte-equivalent across Rust and Java, so downstream tooling (Slack notifiers, Grafana dashboards) reads one shape regardless of where the diff was produced.

## CI tollgate - the GitHub Action

The cookbook ships a [`subms-diff` composite action](https://github.com/submillisecond/subms-cookbook/tree/main/.github/actions/subms-diff) that runs the same diff math at PR time, posts a markdown table as a PR comment, uploads the diff JSON as an artifact, and **fails the status check** when any stage regresses beyond the threshold.

```yaml
name: cookbook perf diff
on:
  pull_request:
    paths: ["cookbook/recipes/**"]

permissions:
  contents: read
  pull-requests: write

jobs:
  diff:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }

      - name: Snapshot baseline
        run: git show ${{ github.event.pull_request.base.sha }}:perf/rust.json > baseline.json

      - name: Use PR candidate
        run: cp perf/rust.json candidate.json

      - name: Diff and gate
        uses: ./.github/actions/subms-diff
        with:
          baseline: baseline.json
          candidate: candidate.json
          threshold-pct: "15"
          fail-on-regression: "true"
```

The status-check fail is the difference between "we have a benchmark" and "we have a contract". JMH leaves you to parse JSON; Criterion has it inside its own reporter; subms makes the gate enforceable in the same place reviewers already look.

## Paced stages - coordinated-omission correction

Default `stage.time(...)` measures one operation in isolation. Some benches need to simulate a constant arrival rate (queues, rate limiters): "what does a client see when the system stalls?" If the loop falls behind the target schedule, an uncorrected bench records only the slow op as slow - the "fast" ops that ran late get reported as fast, hiding real queueing delay. This is [coordinated omission](https://groups.google.com/g/mechanical-sympathy/c/icNZJejUHfE/m/BfDekfBEs_sJ) (Gil Tene's term).

Rust ships `subms::SubMsPacedStage` and Java ships `SubMsPerfHarness.PacedStage`. Both record latency from each op's **intended** start time, not the wall-clock start. Queue delay folds into the per-op number.

**Rust**

```rust
use subms::SubMsPerfHarness;

let mut h = SubMsPerfHarness::new("queue", "rust");
let stage = h.stage("offer", 100_000);
let mut paced = stage.with_pacing(10_000.0);   // target 10k ops/sec

for _ in 0..100_000 {
    paced.time(|| queue.offer(value));
}
```

**Java**

```java
SubMsPerfHarness h = new SubMsPerfHarness("queue", "java");
SubMsPerfHarness.Stage stage = h.stage("offer", 100_000);
SubMsPerfHarness.PacedStage paced = stage.withPacing(10_000.0);

for (int i = 0; i < 100_000; i++) {
    paced.time(() -> queue.offer(value));
}
```

When ops finish in time, `PacedStage` is a no-op (it parks-until-slot but the slot is already past, so it doesn't sleep). When an op overruns, subsequent ops record their corrected slot-delay. The output sample buffer holds the same number of entries as without pacing, so the JSON shape and all downstream tooling work unchanged.

Use it for any bench whose answer to "what's p99?" depends on a target arrival rate. Skip it for benches that measure pure per-call work (no rate semantics).

## Putting it together

A complete cookbook-style bench in 30 lines:

**Rust**

```rust
#![cfg(feature = "harness")]

use subms::{
    assert_p99_under, print_summary, run_bench, summarize_lean,
    SubMsBenchAssertion, SubMsBenchParams,
};

#[test]
fn sub_millisecond_bench() {
    let params = SubMsBenchParams { entries: 50_000, warmup: 5_000, seed: 0 };
    let h = run_bench(&MyRecipe, &params);
    let s = summarize_lean(&h);

    print_summary(&s, &mut std::io::stdout().lock()).unwrap();

    assert_p99_under(&s, &[
        SubMsBenchAssertion { stage: "put", p99_ns_max: 1_000_000 },
    ]).unwrap();
}
```

**Java**

```java
public final class SubmillisecondBench {
    private static final long ONE_MS_NS = 1_000_000L;
    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(50_000, 5_000, 0L);
        SubMsBenchSummary s = SubMsBench.summarizeLean(
                SubMsBench.runBench(new MyRecipe(), params));

        SubMsBench.printSummary(s, System.out);

        SubMsBench.assertP99Under(s, List.of(
                new SubMsBench.Assertion("put", ONE_MS_NS)));
    }
}
```

Same flow, same output format, same gate. That symmetry is the point of subms - the cookbook ships a recipe once, the harness measures it twice.
