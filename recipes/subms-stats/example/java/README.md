# subms-stats - reading a latency distribution

A runnable Java walkthrough of the `subms-stats` library. Take a `long[]`
of nanosecond latency readings (from `SubMsPerfHarness`, JFR, or a raw
`nanoTime` loop) and pass it through the full public surface:
percentiles, tail analysis, robust spread, measurement-rig jitter, A/B
comparison, and a bootstrap CI on the headline percentile.

Features in scope:

- **`SubMsSamples` facade** - the method-chained entry point over a
  sample array. `s.p99()`, `s.jitterScore()`, `s.bootstrapPercentileCi(...)`.
- **Percentiles** - p50, p90, p99, p99.9, max, mean, stddev, and a sweep.
- **Tail** - conditional tail expectation (CTE / CVaR), the Hill tail
  index, and the p99 / p50 fatness ratio.
- **Robust** - IQR, MAD, coefficient of variation, skewness, excess
  kurtosis. The estimators that do not move when one sample blows up.
- **Jitter** - `jitterScore` over non-overlapping 32-sample windows.
  The measurement-rig stability indicator.
- **Compare** - KS statistic and Cohen's d between baseline + candidate.
- **Bootstrap** - confidence interval on a percentile via resampling.

```sh
mvn -q package
mvn -q test                                                                                # JUnit 5: workload determinism + every wrapper output

# Short stdout demo of the SubMsSamples facade against a synthetic sample stream.
java -cp target/classes com.submillisecond.primers.stats.Demo

# Full structured report: baseline vs candidate, KS + Cohen's d, bootstrap CI on p99.
java -cp target/classes com.submillisecond.primers.stats.StatsMain
```

## What `StatsMain` shows

`Workload` generates two synthetic sample streams - a clean baseline and
a heavy-tailed candidate. `Analyser` runs each through `SubMsSamples` +
`Tail` + `Robust` + `Jitter` and returns a `StatsReport` record.
`StatsMain` prints both reports side by side, then asks `Compare` the
A/B question (did the distribution actually move?) and `Bootstrap` the
confidence question (how wide is the CI on the candidate's p99?).

The point of the primer is not the bench numbers - it is the shape of
analysis the library lets you express in 20-ish lines once you have a
sample array in hand. A real bench feeds in the harness output; this
demo feeds in a Workload so the report is reproducible without a CI
runner.

## Files

- `src/main/java/com/submillisecond/primers/stats/Workload.java`
  deterministic synthetic latency generator with a `tailShape` knob.
- `src/main/java/com/submillisecond/primers/stats/StatsReport.java`
  typed record holding the full analysis output.
- `src/main/java/com/submillisecond/primers/stats/Analyser.java`
  takes a `long[]`, returns a `StatsReport`. The end-to-end use of the
  subms-stats public surface.
- `src/main/java/com/submillisecond/primers/stats/StatsMain.java`
  prints two reports + a Compare block + a Bootstrap CI.
- `src/main/java/com/submillisecond/primers/stats/Demo.java`
  the smallest possible "what does `SubMsSamples` give me" snippet.
- `src/test/java/com/submillisecond/primers/stats/WorkloadTest.java`
  JUnit 5 - workload determinism + shape invariants.
- `src/test/java/com/submillisecond/primers/stats/AnalyserTest.java`
  JUnit 5 - every wrapper output, plus the two-run compare detects the
  synthetic regression.
