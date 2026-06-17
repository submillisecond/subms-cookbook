//! Per-feature bench: runs the same 50k-iteration hot-path workload
//! against the base `RateLimiter`, plus each opt-in feature
//! (`token-bucket`, `hierarchical`, `distributed-backend`, `metrics`)
//! when its Cargo feature is enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_try_acquire`, `token_bucket_try_acquire`, etc. - so the
//! cookbook page can fill in the per-feature p99 table from a single
//! JSON file.
//!
//! Every limiter is sized so the grant path dominates: capacity / burst
//! comfortably exceeds the iteration count, so we measure the cost of a
//! successful `try_acquire` (the hot path) rather than the reject path.
//! The feature limiters all run on a real `SystemClock`.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness token-bucket hierarchical distributed-backend metrics"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, bench_indexed_op, summarize, summary_to_json};
use subms_rate_limiter::RateLimiter;

const ITERATIONS: usize = 50_000;
const SEED: u64 = 0;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("rate-limiter-features", "rust");
    h.input("iterations", &ITERATIONS.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-rate-limiter");
    h.add_meta("subms.recipe.category", "scheduling");

    // ---------- base ----------
    h.add_meta("subms.workload.feature", "base");
    // High rate + a burst headroom larger than the loop so every acquire
    // is granted. The base limiter pushes `tat` forward by one period per
    // grant; with wall-clock `now` near-static across a tight loop, the
    // burst window must cover the whole run.
    let base = RateLimiter::new(1_000_000.0, (2 * ITERATIONS) as u64);
    bench_indexed_op(&mut h, "base_try_acquire", ITERATIONS, |_| {
        let _ = base.try_acquire();
    });
    h.stage_mut("base_try_acquire")
        .unwrap()
        .with_kind(SubMsStageKind::HotPath);

    // ---------- token-bucket ----------
    #[cfg(feature = "token-bucket")]
    {
        use subms_rate_limiter::TokenBucket;
        h.add_meta("subms.workload.feature", "token-bucket");
        // Capacity above the iteration count + a high refill so the
        // bucket never empties during the run.
        let tb = TokenBucket::new((2 * ITERATIONS) as u64, 1_000_000.0);
        bench_indexed_op(&mut h, "token_bucket_try_acquire", ITERATIONS, |_| {
            let _ = tb.try_acquire(1);
        });
        h.stage_mut("token_bucket_try_acquire")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- hierarchical ----------
    #[cfg(feature = "hierarchical")]
    {
        use subms_rate_limiter::HierarchicalLimiter;
        h.add_meta("subms.workload.feature", "hierarchical");
        // Parent + single child both sized above the loop so each call
        // clears child AND parent.
        let hier = HierarchicalLimiter::new(
            (2 * ITERATIONS) as u64,
            1_000_000.0,
            1,
            (2 * ITERATIONS) as u64,
            1_000_000.0,
        );
        bench_indexed_op(&mut h, "hierarchical_try_acquire", ITERATIONS, |_| {
            let _ = hier.try_acquire(0, 1);
        });
        h.stage_mut("hierarchical_try_acquire")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- distributed-backend ----------
    #[cfg(feature = "distributed-backend")]
    {
        use subms_rate_limiter::{DistributedLimiter, InMemoryBackend};
        h.add_meta("subms.workload.feature", "distributed-backend");
        // Fixed-window counter; limit above the iteration count and a
        // wide window so all calls land inside one window and grant.
        let backend = Box::new(InMemoryBackend::new());
        let dist = DistributedLimiter::new(backend, (2 * ITERATIONS) as u64, 3_600_000_000_000);
        bench_indexed_op(
            &mut h,
            "distributed_backend_try_acquire",
            ITERATIONS,
            |_| {
                let _ = dist.try_acquire("hot-key");
            },
        );
        h.stage_mut("distributed_backend_try_acquire")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    // ---------- metrics ----------
    #[cfg(feature = "metrics")]
    {
        use subms_rate_limiter::MeteredTokenBucket;
        h.add_meta("subms.workload.feature", "metrics");
        let metered = MeteredTokenBucket::new((2 * ITERATIONS) as u64, 1_000_000.0);
        bench_indexed_op(&mut h, "metrics_try_acquire", ITERATIONS, |_| {
            let _ = metered.try_acquire(1);
        });
        h.stage_mut("metrics_try_acquire")
            .unwrap()
            .with_kind(SubMsStageKind::HotPath);
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
