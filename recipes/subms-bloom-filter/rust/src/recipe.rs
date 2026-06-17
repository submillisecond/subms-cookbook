//! `Recipe` impl for the bloom filter perf workload. Behind the `harness` feature.

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind};

use crate::BloomFilter;

/// Stages: `add`, `might_contain_hit`, `might_contain_miss`.
pub struct BloomFilterRecipe;

impl SubMsRecipe for BloomFilterRecipe {
    fn name(&self) -> &str {
        "bloom-filter"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;

        let mut bf = BloomFilter::new(entries);

        // Warm-up so icache fill / branch warm-up don't leak into steady state.
        for i in 0..warmup {
            bf.add(&format!("warm{i}"));
        }

        {
            let s = h.stage("add", entries).with_kind(SubMsStageKind::HotPath);
            for i in 0..entries {
                let key = format!("key{i}");
                s.time(|| {
                    bf.add(&key);
                });
            }
        }

        {
            let s = h
                .stage("might_contain_hit", entries)
                .with_kind(SubMsStageKind::HotPath);
            let mut rng = SubMsLcg::new(seed);
            for _ in 0..entries {
                let key = format!("key{}", rng.bounded(entries as u32));
                s.time(|| {
                    let _ = bf.might_contain(&key);
                });
            }
        }

        {
            let s = h
                .stage("might_contain_miss", entries)
                .with_kind(SubMsStageKind::HotPath);
            let mut rng = SubMsLcg::new(seed.wrapping_add(1));
            for _ in 0..entries {
                let key = format!("absent{}", rng.bounded(entries as u32 * 10));
                s.time(|| {
                    let _ = bf.might_contain(&key);
                });
            }
        }

        h.add_meta("bits_per_key", "10");
        h.add_meta("k", "7");
    }
}
