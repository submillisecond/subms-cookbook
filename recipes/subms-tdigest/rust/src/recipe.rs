//! `SubMsRecipe` impl - add (streaming ingest), quantile (query), merge.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsTDigest;

pub struct TDigestRecipe;

const COMPRESSION: f64 = 100.0;
const MERGE_ROUNDS: usize = 2_000;

impl SubMsRecipe for TDigestRecipe {
    fn name(&self) -> &str {
        "subms-tdigest"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;

        // ---------- add: stream values in ----------
        let mut d = TsTDigest::new(COMPRESSION);
        let s_add = h.stage("add", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed);
        for _ in 0..entries {
            let v = (rng.next_u32() >> 8) as f64 / 65_536.0;
            let t0 = SubMsTimer::tick();
            d.add(v);
            s_add.record(t0.elapsed_ns());
        }
        d.compact();

        // ---------- quantile: query percentiles ----------
        let qs = [0.5, 0.9, 0.99, 0.999];
        let s_q = h
            .stage("quantile", entries)
            .with_kind(SubMsStageKind::HotPath);
        let mut rng2 = SubMsLcg::new(seed ^ 0x77);
        for _ in 0..entries {
            let q = qs[(rng2.next_u32() % qs.len() as u32) as usize];
            let t0 = SubMsTimer::tick();
            let v = d.quantile(q);
            s_q.record(t0.elapsed_ns());
            std::hint::black_box(v);
        }

        // ---------- merge: fold two sketches ----------
        let mut a = TsTDigest::new(COMPRESSION);
        let mut b = TsTDigest::new(COMPRESSION);
        let mut rng3 = SubMsLcg::new(seed ^ 0x55);
        for _ in 0..100_000 {
            a.add((rng3.next_u32() >> 8) as f64);
            b.add((rng3.next_u32() >> 8) as f64);
        }
        a.compact();
        b.compact();
        let s_m = h
            .stage("merge", MERGE_ROUNDS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..MERGE_ROUNDS {
            let t0 = SubMsTimer::tick();
            let m = a.merge(&b);
            s_m.record(t0.elapsed_ns());
            std::hint::black_box(m.count());
        }

        h.add_meta("compression", &COMPRESSION.to_string());
        h.add_meta("serialized_bytes", &d.serialize().len().to_string());
        h.add_meta("subms.workload.feature", "streaming-quantile");
    }
}
