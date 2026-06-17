//! `SubMsRecipe` impl - backward + nearest asof joins over two N-point series.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::TsSeries;

use crate::{asof_join_backward, asof_join_nearest};

pub struct AsofJoinRecipe;

// A join is one operation over two whole series; bench it at a realistic
// per-call series size, repeated, rather than per-point.
const SERIES: usize = 1_024;

impl SubMsRecipe for AsofJoinRecipe {
    fn name(&self) -> &str {
        "subms-ts-asof-join"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let seed = params.seed;

        let mut rng = SubMsLcg::new(seed);
        let mut left = TsSeries::<f64>::with_capacity(SERIES);
        let mut right = TsSeries::<f64>::with_capacity(SERIES);
        for i in 0..SERIES as i64 {
            let _ = left.push(i * 3, (rng.next_u32() >> 16) as f64);
            let _ = right.push(i * 2, (rng.next_u32() >> 16) as f64);
        }

        let s_back = h
            .stage("join_backward", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let m = asof_join_backward(&left, &right);
            s_back.record(t0.elapsed_ns());
            std::hint::black_box(m.len());
        }

        let s_near = h
            .stage("join_nearest", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let m = asof_join_nearest(&left, &right, 4);
            s_near.record(t0.elapsed_ns());
            std::hint::black_box(m.len());
        }

        h.add_meta("series_points", &SERIES.to_string());
        h.add_meta("subms.workload.feature", "asof-join");
    }
}
