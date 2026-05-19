//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

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

        let s_add = h.stage("add", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let key = format!("k{}", rng.bounded(1000));
            let t0 = Instant::now();
            cms.add(&key);
            s_add.record(t0.elapsed().as_nanos() as u64);
        }

        let s_q = h.stage("estimate", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(2));
        for _ in 0..entries {
            let key = format!("k{}", rng.bounded(1000));
            let t0 = Instant::now();
            let _ = cms.estimate(&key);
            s_q.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("d", &cms.depth().to_string());
        h.add_meta("w", &cms.width().to_string());
    }
}
