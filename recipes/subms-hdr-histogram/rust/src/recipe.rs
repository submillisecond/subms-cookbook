//! `SubMsRecipe` impl.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::HdrHistogram;

pub struct HdrHistogramRecipe;

impl SubMsRecipe for HdrHistogramRecipe {
    fn name(&self) -> &str {
        "hdr-histogram"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let mut hist = HdrHistogram::new(3);

        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            hist.record(rng.next_u32() as u64 + 1);
        }

        let s_rec = h
            .stage("record", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let v = (rng.next_u32() as u64 % 1_000_000) + 1;
            let t0 = SubMsTimer::tick();
            hist.record(v);
            s_rec.record(t0.elapsed_ns());
        }

        let s_p = h
            .stage("percentile", 100)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..100 {
            let t0 = SubMsTimer::tick();
            let _ = std::hint::black_box(hist.value_at_percentile(0.99));
            s_p.record(t0.elapsed_ns());
        }

        h.add_meta("p99_ns", &hist.value_at_percentile(0.99).to_string());
        h.add_meta("max_ns", &hist.max().to_string());
    }
}
