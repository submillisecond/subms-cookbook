//! `SubMsRecipe` impl - apply a retention policy over a freshly grown series.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::TsSeries;

use crate::TsRetentionPolicy;

pub struct RetentionRecipe;

const SERIES: usize = 4_096;

impl SubMsRecipe for RetentionRecipe {
    fn name(&self) -> &str {
        "subms-ts-retention"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let seed = params.seed;
        let mut rng = SubMsLcg::new(seed);

        // age policy keeps the newest ~half; count policy keeps the newest 1k.
        let by_age = TsRetentionPolicy::new().max_age_ns(SERIES as i64 / 2);
        let by_count = TsRetentionPolicy::new().max_points(1_024);

        let s_age = h
            .stage("apply_age", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let mut s = TsSeries::<f64>::with_capacity(SERIES);
            for i in 0..SERIES {
                let _ = s.push(i as i64, (rng.next_u32() >> 16) as f64);
            }
            let t0 = SubMsTimer::tick();
            let removed = by_age.apply(&mut s);
            s_age.record(t0.elapsed_ns());
            std::hint::black_box(removed);
        }

        let s_cnt = h
            .stage("apply_count", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let mut s = TsSeries::<f64>::with_capacity(SERIES);
            for i in 0..SERIES {
                let _ = s.push(i as i64, (rng.next_u32() >> 16) as f64);
            }
            let t0 = SubMsTimer::tick();
            let removed = by_count.apply(&mut s);
            s_cnt.record(t0.elapsed_ns());
            std::hint::black_box(removed);
        }

        h.add_meta("series_points", &SERIES.to_string());
        h.add_meta("subms.workload.feature", "retention");
    }
}
