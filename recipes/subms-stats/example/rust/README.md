# subms-primer-stats (Rust)

The runnable companion to the `subms-stats` primer. A synthetic latency
workload with a configurable tail shape, driven through every public
analysis in `subms-stats` end-to-end: the `SubMsSamples` facade,
percentiles, tail (CTE, Hill, fatness), robust (IQR, MAD, CoV, skew,
kurtosis), jitter, log2 CDF buckets, two-sample KS + Cohen's d, and a
bootstrap CI on p99.

This crate is not published. It exists to be read, run, and copied from
when you start your own analysis pipeline.

## What it shows

- `Workload` - a deterministic synthetic latency-sample generator with two
  tail regimes (`Uniformish`, `PowerLaw`). Models the "quiet hot path"
  shape and the "GC pause / scheduler-pre-empted" shape respectively.
- `analyse(&[u64]) -> StatsReport` - runs the full `subms-stats` surface
  over one sample stream and returns a typed bundle in the same order the
  primer writeup introduces the analyses.
- `compare(&[u64], &[u64]) -> CompareReport` - the two-run story:
  KS statistic, Cohen's d, plus a bootstrap CI on each p99 so the reader
  can see whether the comparison verdict is supported by non-overlapping
  intervals.

## Run the example

```sh
cargo run --release --example stats_main
```

Prints two per-run reports (a `Uniformish` baseline and a `PowerLaw`
candidate) followed by a `Compare` block and an editorial verdict.

## Run the tests

```sh
cargo test --release
```

Covers workload determinism, every wrapper analysis lands in a sensible
range, and the two-run comparison reliably detects the synthetic
power-law regression.

## Layout

- `src/lib.rs` - `Workload`, `TailShape`, `analyse`, `compare`,
  `StatsReport`, `CompareReport`, default bootstrap parameters.
- `examples/stats_main.rs` - the end-to-end driver that prints the two
  reports + the comparison block.
- `tests/lib_tests.rs` - workload, analysis, and comparison tests.

## License

MIT OR Apache-2.0.
