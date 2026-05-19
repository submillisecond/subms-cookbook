//! `SubMsRecipe` impl for the LSM tree perf workload. Behind the `harness` feature.

use std::env;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use subms::{SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe};

use crate::{BloomMode, LsmTree};

static DIR_SEQ: AtomicU64 = AtomicU64::new(0);

/// Stages: `put`, `get_hit`, `get_miss`. Opens a fresh LSM tree under a temp dir.
pub struct LsmTreeRecipe {
    /// Memtable byte threshold before flushing to an SSTable.
    pub flush_threshold_bytes: usize,
    /// On = consult the per-SSTable bloom filter before scanning; Off = skip.
    pub bloom_mode: BloomMode,
}

impl LsmTreeRecipe {
    pub fn new(flush_threshold_bytes: usize, bloom_mode: BloomMode) -> Self {
        Self {
            flush_threshold_bytes,
            bloom_mode,
        }
    }
}

fn fresh_dir() -> PathBuf {
    let n = DIR_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = env::temp_dir().join(format!("lsm-recipe-{}-{}", std::process::id(), n));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

impl SubMsRecipe for LsmTreeRecipe {
    fn name(&self) -> &str {
        "lsm-tree"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let warmup = params.warmup;
        let seed = params.seed;

        h.input(
            "flush_threshold_bytes",
            &self.flush_threshold_bytes.to_string(),
        );
        h.input(
            "bloom_mode",
            match self.bloom_mode {
                BloomMode::On => "on",
                BloomMode::Off => "off",
            },
        );

        let dir = fresh_dir();
        let sstables = {
            let mut lsm = LsmTree::open_with(&dir, self.flush_threshold_bytes, self.bloom_mode)
                .expect("open lsm");

            for i in 0..warmup {
                lsm.put(&format!("warm{i}"), format!("v{i}").as_bytes())
                    .unwrap();
            }
            for i in 0..warmup {
                let _ = lsm.get(&format!("warm{i}")).unwrap();
            }

            {
                let s = h.stage("put", entries);
                for i in 0..entries {
                    let k = format!("key{i}");
                    let v = format!("v{i}");
                    s.time(|| {
                        lsm.put(&k, v.as_bytes()).unwrap();
                    });
                }
            }

            {
                let mut rng = SubMsLcg::new(seed);
                let s = h.stage("get_hit", entries);
                for _ in 0..entries {
                    let k = format!("key{}", rng.bounded(entries as u32));
                    s.time(|| {
                        let _ = lsm.get(&k).unwrap();
                    });
                }
            }

            {
                let mut rng = SubMsLcg::new(seed.wrapping_add(1));
                let s = h.stage("get_miss", entries);
                for _ in 0..entries {
                    let k = format!("missing{}", rng.bounded(entries as u32 * 10));
                    s.time(|| {
                        let _ = lsm.get(&k).unwrap();
                    });
                }
            }

            lsm.sstable_count()
        };

        h.add_meta("sstables", &sstables.to_string());
        let _ = fs::remove_dir_all(&dir);
    }
}
