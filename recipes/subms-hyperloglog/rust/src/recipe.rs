//! `SubMsRecipe` impl. Behind the `harness` feature.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::HyperLogLog;

/// Stages: `add`, `estimate`.
pub struct HyperLogLogRecipe;

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

        let s_add = h.stage("add", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let key = format!("k{}", rng.next_u32());
            let t0 = Instant::now();
            hll.add(&key);
            s_add.record(t0.elapsed().as_nanos() as u64);
        }

        let s_est = h.stage("estimate", 100);
        for _ in 0..100 {
            let t0 = Instant::now();
            let _ = hll.estimate();
            s_est.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("precision", "14");
        h.add_meta("registers", &hll.register_count().to_string());
        h.add_meta("estimate", &(hll.estimate() as u64).to_string());
    }
}
