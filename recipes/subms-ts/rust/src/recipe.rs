//! `SubMsRecipe` impl - drives a representative `subms-ts` workload over a
//! scalar `f64` series: build (push), point lookup (nearest), and two ranged
//! aggregates over a fixed window.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsSeries;

pub struct TsRecipe;

const WINDOW: i64 = 256;

impl SubMsRecipe for TsRecipe {
    fn name(&self) -> &str {
        "subms-ts"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;

        // ---------- push: build a dense 1-point-per-tick series ----------
        let mut s = TsSeries::<f64>::with_capacity(entries);
        let s_push = h.stage("push", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed);
        for i in 0..entries {
            let v = rng.next_u32() as f64 / u32::MAX as f64;
            let t0 = SubMsTimer::tick();
            s.push(i as i64, v).unwrap();
            s_push.record(t0.elapsed_ns());
        }

        let span = entries.max(1) as i64;
        let max_start = (span - WINDOW - 1).max(0);

        // ---------- nearest: random as-of lookups ----------
        let s_near = h
            .stage("nearest", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed ^ 0x1234);
        for _ in 0..entries {
            let target = (rng.next_u32() as i64).rem_euclid(span);
            let t0 = SubMsTimer::tick();
            let _ = s.nearest(target);
            s_near.record(t0.elapsed_ns());
        }

        // ---------- range_min over a fixed window ----------
        let s_rmin = h
            .stage("range_min", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed ^ 0x5678);
        for _ in 0..entries {
            let from = (rng.next_u32() as i64).rem_euclid(max_start + 1);
            let t0 = SubMsTimer::tick();
            let _ = s.range_min(from, from + WINDOW);
            s_rmin.record(t0.elapsed_ns());
        }

        // ---------- range_sum over a fixed window ----------
        let s_rsum = h
            .stage("range_sum", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed ^ 0x9abc);
        for _ in 0..entries {
            let from = (rng.next_u32() as i64).rem_euclid(max_start + 1);
            let t0 = SubMsTimer::tick();
            let _ = s.range_sum(from, from + WINDOW);
            s_rsum.record(t0.elapsed_ns());
        }

        h.add_meta("len", &s.len().to_string());
        h.add_meta("subms.workload.feature", "scalar-f64");
    }
}
