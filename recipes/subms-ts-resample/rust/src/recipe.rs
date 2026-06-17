//! `SubMsRecipe` impl - resample an irregular series to a grid (mean + last).

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::TsSeries;

use crate::{TsResampleMode, resample_to_grid};

pub struct ResampleRecipe;

const SERIES: usize = 1_024;
const PERIOD: i64 = 100;

impl SubMsRecipe for ResampleRecipe {
    fn name(&self) -> &str {
        "subms-ts-resample"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let seed = params.seed;

        let mut rng = SubMsLcg::new(seed);
        let mut s = TsSeries::<f64>::with_capacity(SERIES);
        let mut ts = 0i64;
        for _ in 0..SERIES {
            let _ = s.push(ts, (rng.next_u32() >> 16) as f64);
            ts += 10 + (rng.next_u32() % 40) as i64;
        }

        let s_mean = h
            .stage("resample_mean", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let g = resample_to_grid(&s, PERIOD, TsResampleMode::Mean);
            s_mean.record(t0.elapsed_ns());
            std::hint::black_box(g.len());
        }

        let s_last = h
            .stage("resample_last", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let g = resample_to_grid(&s, PERIOD, TsResampleMode::Last);
            s_last.record(t0.elapsed_ns());
            std::hint::black_box(g.len());
        }

        h.add_meta("series_points", &SERIES.to_string());
        h.add_meta("subms.workload.feature", "grid-resample");
    }
}
