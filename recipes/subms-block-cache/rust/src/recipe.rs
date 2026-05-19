//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::BlockCache;

pub struct BlockCacheRecipe;

impl SubMsRecipe for BlockCacheRecipe {
    fn name(&self) -> &str {
        "block-cache"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let cap = 1024;
        let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(cap);

        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            let k = rng.bounded((cap * 2) as u32);
            c.put(k, k);
        }

        let s_get = h.stage("get", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(1));
        for _ in 0..entries {
            let k = rng.bounded((cap * 2) as u32);
            let t0 = Instant::now();
            let _ = c.get(&k);
            s_get.record(t0.elapsed().as_nanos() as u64);
        }

        let s_put = h.stage("put", entries);
        let mut rng = SubMsLcg::new(seed.wrapping_add(2));
        for _ in 0..entries {
            let k = rng.bounded((cap * 4) as u32);
            let t0 = Instant::now();
            c.put(k, k);
            s_put.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("capacity", &cap.to_string());
    }
}
