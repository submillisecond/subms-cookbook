//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::Art;

pub struct ArtRecipe;

impl SubMsRecipe for ArtRecipe {
    fn name(&self) -> &str { "adaptive-radix-tree" }

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

        let s_ins = h.stage("insert", entries);
        let mut keys: Vec<String> = Vec::with_capacity(entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let key = format!("k{}", rng.next_u32());
            keys.push(key.clone());
            let t0 = Instant::now();
            t.insert(key.as_bytes(), 0);
            s_ins.record(t0.elapsed().as_nanos() as u64);
        }

        let s_get = h.stage("lookup", entries);
        for key in &keys {
            let t0 = Instant::now();
            let _ = t.get(key.as_bytes());
            s_get.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("len", &t.len().to_string());
    }
}
