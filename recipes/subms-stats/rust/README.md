# subms-stats

Latency-distribution statistics. Pure functions on `&[u64]` sample
arrays plus an ergonomic `SubMsSamples` facade.

This is the statistics primitive the `subms` perf harness uses
internally, published as its own crate so you can use it without
pulling in the bench machinery. Bring your own `Vec<u64>` of
nanosecond readings (from any source) and compose the analyses you
need.

```sh
cargo add subms-stats
```

## Modules / Cargo features

| Feature | Module | What |
|---|---|---|
| (always on) | `percentiles` | `percentile`, `percentile_sweep`, `mean`, `stddev` |
| `histogram` | `histogram` | 64-bucket log2 CDF for full-distribution export |
| `jitter` | `jitter` | Per-window CV score - measurement-rig stability |
| `tail` | `tail` | Conditional tail expectation, Hill estimator, p99/p50 fatness |
| `robust` | `robust` | IQR, MAD, CoV, skewness, excess kurtosis |
| `compare` | `compare` | KS statistic, Cohen's d effect size |
| `bootstrap` | `bootstrap` | Reproducible bootstrap CIs for percentiles |

All features are on by default. For a minimal build:

```toml
subms-stats = { version = "0.5", default-features = false }
```

For a focused subset:

```toml
subms-stats = { version = "0.5", default-features = false, features = ["histogram", "tail"] }
```

## Quickstart

```rust
use subms_stats::SubMsSamples;

let raw: Vec<u64> = vec![100, 200, 150, 300, 250, 175, 125, 400];
let s = SubMsSamples::new(&raw);

let p99 = s.p99();
let p99_to_p50_ratio = s.tail_fatness_ratio();  // tail feature
let cdf = s.cdf_buckets();                        // histogram feature
let (lo, hi) = s.bootstrap_percentile_ci(0.99, 200, 0.95, 42);  // bootstrap feature
```

## License

Dual: MIT OR Apache-2.0.
