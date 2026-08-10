//! `SubMsRecipe` impl. Behind the `harness` feature.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::HyperLogLog;

/// Stages: `add`, `estimate`.
pub struct HyperLogLogRecipe;

/// Samples for the `estimate` stage. The harness takes p99 as
/// `sorted[floor(0.99 * n)]`, so at n <= 100 that index is `n - 1` and the
/// reported p99 is whichever single call caught a scheduler hiccup. 2000 puts
/// 19 samples above the p99 index and 1 above p999. Do not lower it.
const ESTIMATE_SAMPLES: usize = 2_000;

/// Untimed `estimate` reps before the timed loop. One fold of the register
/// array is far above the per-key budget, so it needs its own warm-up rather
/// than inheriting the one `add` did.
const ESTIMATE_WARM: usize = 64;

impl SubMsRecipe for HyperLogLogRecipe {
    fn name(&self) -> &str {
        "hyperloglog"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let mut hll = HyperLogLog::new(14);

        // Warm-up
        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            hll.add(&format!("warm{}", rng.next_u32()));
        }

        let s_add = h.stage("add", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let key = format!("k{}", rng.next_u32());
            let t0 = SubMsTimer::tick();
            hll.add(&key);
            s_add.record(t0.elapsed_ns());
        }

        for _ in 0..ESTIMATE_WARM {
            std::hint::black_box(hll.estimate());
        }
        let s_est = h
            .stage("estimate", ESTIMATE_SAMPLES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ESTIMATE_SAMPLES {
            let t0 = SubMsTimer::tick();
            // estimate() is pure and in-crate, so dropping the result lets LLVM
            // delete the whole 16k-register scan and time an empty region.
            std::hint::black_box(hll.estimate());
            s_est.record(t0.elapsed_ns());
        }

        h.add_meta("precision", "14");
        h.add_meta("registers", &hll.register_count().to_string());
        h.add_meta("estimate", &(hll.estimate() as u64).to_string());
    }
}
