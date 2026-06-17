//! `SubMsRecipe` impl - record zones (observe) + prune a 100k-zone index
//! (candidates). The pruning claim is the headline: skip blocks a query
//! cannot touch in well under a millisecond.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::{TsZone, TsZoneMap};

pub struct ZoneMapRecipe;

const INDEX_BLOCKS: u64 = 100_000;
const WINDOW: i64 = 2_500;

fn zone(id: u64, value_max: f64) -> TsZone {
    let base = id as i64 * 1_000;
    TsZone {
        block_id: id,
        ts_min: base,
        ts_max: base + 999,
        value_min: 0.0,
        value_max,
        count: 1_000,
    }
}

impl SubMsRecipe for ZoneMapRecipe {
    fn name(&self) -> &str {
        "subms-zone-map"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;

        // A 100k-zone index to prune against (built untimed).
        let mut index = TsZoneMap::with_capacity(INDEX_BLOCKS as usize);
        for id in 0..INDEX_BLOCKS {
            index.observe_zone(zone(id, (id % 500) as f64));
        }

        // ---------- observe: record a zone ----------
        let s_obs = h
            .stage("observe", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut scratch = TsZoneMap::with_capacity(entries);
        let mut rng = SubMsLcg::new(seed);
        for i in 0..entries {
            let z = zone(i as u64, (rng.next_u32() % 500) as f64);
            let t0 = SubMsTimer::tick();
            scratch.observe_zone(z);
            s_obs.record(t0.elapsed_ns());
        }

        // ---------- candidates: prune the 100k index ----------
        let span = INDEX_BLOCKS as i64 * 1_000;
        let s_cand = h
            .stage("candidates", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng2 = SubMsLcg::new(seed ^ 0x99);
        for _ in 0..entries {
            let lo = (rng2.next_u32() as i64).rem_euclid(span);
            let t0 = SubMsTimer::tick();
            let c = index.candidates(lo, lo + WINDOW, None).len();
            s_cand.record(t0.elapsed_ns());
            std::hint::black_box(c);
        }

        h.add_meta("index_blocks", &INDEX_BLOCKS.to_string());
        h.add_meta("subms.workload.feature", "time-window-prune");
    }
}
