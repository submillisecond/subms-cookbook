//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::CuckooFilter;

pub struct CuckooFilterRecipe;

impl SubMsRecipe for CuckooFilterRecipe {
    fn name(&self) -> &str { "cuckoo-filter" }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;
        let mut cf = CuckooFilter::with_capacity(entries);

        let mut rng = SubMsLcg::new(seed);
        for _ in 0..warmup {
            cf.insert(&format!("warm{}", rng.next_u32()));
        }

        let s_ins = h.stage("insert", entries);
        let mut keys = Vec::with_capacity(entries);
        for i in 0..entries {
            let key = format!("k{i}");
            keys.push(key.clone());
            let t0 = Instant::now();
            cf.insert(&key);
            s_ins.record(t0.elapsed().as_nanos() as u64);
        }

        let s_contains = h.stage("contains", entries);
        for key in &keys {
            let t0 = Instant::now();
            let _ = cf.contains(key);
            s_contains.record(t0.elapsed().as_nanos() as u64);
        }

        let s_del = h.stage("delete", entries);
        for key in &keys {
            let t0 = Instant::now();
            let _ = cf.delete(key);
            s_del.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("buckets", &cf.bucket_count().to_string());
    }
}
