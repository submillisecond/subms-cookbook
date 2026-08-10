//! `SubMsRecipe` impl.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::Art;

pub struct ArtRecipe;

impl SubMsRecipe for ArtRecipe {
    fn name(&self) -> &str {
        "adaptive-radix-tree"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let mut t: Art<u32> = Art::new();

        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            let key = format!("k{}", rng.next_u32());
            t.insert(key.as_bytes(), 0);
        }

        let s_ins = h
            .stage("insert", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut keys: Vec<String> = Vec::with_capacity(entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let key = format!("k{}", rng.next_u32());
            keys.push(key.clone());
            let t0 = SubMsTimer::tick();
            t.insert(key.as_bytes(), 0);
            s_ins.record(t0.elapsed_ns());
        }

        let s_get = h
            .stage("lookup", entries)
            .with_kind(SubMsStageKind::HotPath);
        for key in &keys {
            let t0 = SubMsTimer::tick();
            // get() is pure and in-crate; dropping the result makes the whole
            // root-to-leaf descent eliminable.
            std::hint::black_box(t.get(key.as_bytes()));
            s_get.record(t0.elapsed_ns());
        }

        h.add_meta("len", &t.len().to_string());
    }
}
