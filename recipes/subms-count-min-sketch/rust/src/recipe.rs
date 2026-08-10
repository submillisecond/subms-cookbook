//! `SubMsRecipe` impl.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::CountMinSketch;

pub struct CountMinSketchRecipe;

impl SubMsRecipe for CountMinSketchRecipe {
    fn name(&self) -> &str {
        "count-min-sketch"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let mut cms = CountMinSketch::new(5, 16384);

        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            cms.add(&format!("warm{}", rng.next_u32()));
        }

        let s_add = h.stage("add", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let key = format!("k{}", rng.bounded(1000));
            let t0 = SubMsTimer::tick();
            cms.add(&key);
            s_add.record(t0.elapsed_ns());
        }

        let s_q = h
            .stage("estimate", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed.wrapping_add(2));
        for _ in 0..entries {
            let key = format!("k{}", rng.bounded(1000));
            let t0 = SubMsTimer::tick();
            // estimate() is pure and in-crate; dropping the result makes the
            // whole d-row probe eliminable.
            std::hint::black_box(cms.estimate(&key));
            s_q.record(t0.elapsed_ns());
        }

        h.add_meta("d", &cms.depth().to_string());
        h.add_meta("w", &cms.width().to_string());
    }
}
