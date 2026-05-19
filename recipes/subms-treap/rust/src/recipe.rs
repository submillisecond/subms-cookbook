//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::Treap;

pub struct TreapRecipe;

impl SubMsRecipe for TreapRecipe {
    fn name(&self) -> &str { "treap" }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let mut t: Treap<u32, u32> = Treap::new(seed);

        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            let k = rng.next_u32();
            t.insert(k, k);
        }

        let s_ins = h.stage("insert", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        let mut keys = Vec::with_capacity(entries);
        for _ in 0..entries {
            let k = rng.next_u32();
            keys.push(k);
            let t0 = Instant::now();
            t.insert(k, k);
            s_ins.record(t0.elapsed().as_nanos() as u64);
        }

        let s_get = h.stage("lookup", entries);
        for k in &keys {
            let t0 = Instant::now();
            let _ = t.get(k);
            s_get.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("len", &t.len().to_string());
    }
}
