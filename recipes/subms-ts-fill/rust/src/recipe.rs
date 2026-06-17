//! `SubMsRecipe` impl - linear + locf fill over a gappy series.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::TsSeries;

use crate::{fill_linear, fill_locf};

pub struct FillRecipe;

const SERIES: usize = 1_024;
const STEP: i64 = 10;

impl SubMsRecipe for FillRecipe {
    fn name(&self) -> &str {
        "subms-ts-fill"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let seed = params.seed;

        // a gappy series: points every ~35 ns so step-10 fill inserts ~2/gap
        let mut rng = SubMsLcg::new(seed);
        let mut s = TsSeries::<f64>::with_capacity(SERIES);
        let mut ts = 0i64;
        for _ in 0..SERIES {
            let _ = s.push(ts, (rng.next_u32() >> 16) as f64);
            ts += 30 + (rng.next_u32() % 20) as i64;
        }

        let s_lin = h
            .stage("fill_linear", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let f = fill_linear(&s, STEP);
            s_lin.record(t0.elapsed_ns());
            std::hint::black_box(f.len());
        }

        let s_locf = h
            .stage("fill_locf", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let f = fill_locf(&s, STEP);
            s_locf.record(t0.elapsed_ns());
            std::hint::black_box(f.len());
        }

        h.add_meta("series_points", &SERIES.to_string());
        h.add_meta("subms.workload.feature", "gap-fill");
    }
}
