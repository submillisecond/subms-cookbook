//! `SubMsRecipe` impl - push (feeds every tier) + tier-stats query.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsDownsampler;

pub struct DownsamplerRecipe;

// 1s / 1m / 1h tiers over nanosecond timestamps.
const TIERS: [i64; 3] = [1_000_000_000, 60_000_000_000, 3_600_000_000_000];
const STEP_NS: i64 = 100_000_000; // a point every 100 ms

impl SubMsRecipe for DownsamplerRecipe {
    fn name(&self) -> &str {
        "subms-ts-downsampler"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;

        // ---------- push: feed raw points into the 3-tier pipeline ----------
        let mut d = TsDownsampler::new(&TIERS);
        let s_push = h.stage("push", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed);
        let mut ts = 0i64;
        for _ in 0..entries {
            let v = (rng.next_u32() >> 8) as f64 / 65_536.0;
            let t0 = SubMsTimer::tick();
            d.push(ts, v);
            s_push.record(t0.elapsed_ns());
            ts += STEP_NS;
        }
        d.flush();

        // ---------- bucket_stats: query a tier ----------
        let span = ts.max(1);
        let s_q = h
            .stage("bucket_stats", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng2 = SubMsLcg::new(seed ^ 0x33);
        for _ in 0..entries {
            let level = (rng2.next_u32() % TIERS.len() as u32) as usize;
            let at = (rng2.next_u32() as i64).rem_euclid(span);
            let t0 = SubMsTimer::tick();
            let s = d.bucket_stats(level, at);
            s_q.record(t0.elapsed_ns());
            std::hint::black_box(s);
        }

        h.add_meta("tier0_buckets", &d.tier(0).len().to_string());
        h.add_meta("subms.workload.feature", "tiered-rollup");
    }
}
