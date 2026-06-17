//! `SubMsRecipe` impl - append (encode), full-block decode, and a windowed
//! range scan over a Gorilla block.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsGorillaBlock;

pub struct GorillaRecipe;

// A Gorilla block is a sealed column chunk, not a whole series - real blocks
// seal at a few thousand points. The decode + scan claims are stated per
// block at this size; appends are O(1) regardless.
const BLOCK_POINTS: usize = 1_024;
const WINDOW: i64 = 256;

fn gauge(rng: &mut SubMsLcg) -> f64 {
    20.0 + (rng.next_u32() >> 28) as f64
}

impl SubMsRecipe for GorillaRecipe {
    fn name(&self) -> &str {
        "subms-gorilla-block"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;
        let base_ts = 1_700_000_000i64;

        // ---------- append: O(1) per point, resealing into block-sized chunks ----------
        let s_app = h
            .stage("append", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed);
        let mut blk = TsGorillaBlock::with_capacity(BLOCK_POINTS * 2);
        for i in 0..entries {
            if blk.len() >= BLOCK_POINTS {
                blk = TsGorillaBlock::with_capacity(BLOCK_POINTS * 2);
            }
            let v = gauge(&mut rng);
            let t0 = SubMsTimer::tick();
            blk.append(base_ts + i as i64, v);
            s_app.record(t0.elapsed_ns());
        }

        // A reference block of one sealed chunk, for the read-path stages.
        let mut refb = TsGorillaBlock::with_capacity(BLOCK_POINTS * 2);
        let mut rng2 = SubMsLcg::new(seed ^ 0x55);
        for i in 0..BLOCK_POINTS {
            refb.append(base_ts + i as i64, gauge(&mut rng2));
        }
        let refbytes = refb.bytes();
        let rounds = entries; // record as many read ops as the append count

        // ---------- decode a whole block ----------
        let s_dec = h.stage("decode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let pts = TsGorillaBlock::decode(&refbytes).unwrap();
            s_dec.record(t0.elapsed_ns());
            std::hint::black_box(pts.len());
        }

        // ---------- windowed range scan ----------
        let s_rng = h
            .stage("range_scan", rounds)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng3 = SubMsLcg::new(seed ^ 0xdead);
        for _ in 0..rounds {
            let from = base_ts + (rng3.next_u32() as i64).rem_euclid(BLOCK_POINTS as i64);
            let t0 = SubMsTimer::tick();
            let c = refb.range(from, from + WINDOW).count();
            s_rng.record(t0.elapsed_ns());
            std::hint::black_box(c);
        }

        h.add_meta("block_points", &BLOCK_POINTS.to_string());
        h.add_meta("block_bytes", &refbytes.len().to_string());
        h.add_meta(
            "bytes_per_point",
            &format!("{:.3}", refbytes.len() as f64 / BLOCK_POINTS as f64),
        );
        h.add_meta("subms.workload.feature", "scalar-f64");
    }
}
