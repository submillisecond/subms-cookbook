//! `SubMsRecipe` impl - two per-op stages. `encode` maps ONE point to its BSON
//! document; `decode` parses one document back. That single-point map is the
//! tick-loop primitive and the asserted sub-ms claim. The network round trip
//! (the driver) is not benched - it is network-bound and reported, not claimed;
//! whole-batch bulk throughput is reported in the writeup, not asserted here.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsSeries, TsSeriesMetadata};

use crate::{doc_from_bytes, doc_to_bytes, point_doc, point_from_doc};

pub struct MongoRecipe;

const POINTS: usize = 4_096;

fn build_series(seed: u64) -> TsSeries<f64> {
    let mut rng = SubMsLcg::new(seed);
    let meta = TsSeriesMetadata::new(1, "cpu")
        .with_tag("host", "edge-01")
        .with_tag("region", "us-east-1");
    let mut s = TsSeries::<f64>::with_capacity(POINTS);
    let base = 1_780_000_000_000_000_000i64;
    for i in 0..POINTS {
        let v = ((rng.next_u32() >> 12) as f64) / 1000.0;
        let _ = s.push(base + i as i64 * 1_000_000_000, v);
    }
    s.with_metadata(meta)
}

impl SubMsRecipe for MongoRecipe {
    fn name(&self) -> &str {
        "subms-ts-adapter-mongodb"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let series = build_series(params.seed);
        // The per-op primitive a tick loop runs: encode / decode ONE point
        // document. That is the asserted sub-ms claim. Whole-batch throughput
        // is reported in the writeup, not benched as a single op.
        let pts: Vec<(i64, f64)> = series.iter().map(|p| (p.ts, p.value)).collect();

        let s_encode = h.stage("encode", rounds).with_kind(SubMsStageKind::HotPath);
        for r in 0..rounds {
            let (ts, v) = pts[r % pts.len()];
            let t0 = SubMsTimer::tick();
            let bytes = doc_to_bytes(&point_doc(1, ts, v)).expect("encode");
            s_encode.record(t0.elapsed_ns());
            std::hint::black_box(bytes.len());
        }

        let encoded: Vec<Vec<u8>> = pts
            .iter()
            .map(|(ts, v)| doc_to_bytes(&point_doc(1, *ts, *v)).expect("encode"))
            .collect();

        let s_decode = h.stage("decode", rounds).with_kind(SubMsStageKind::HotPath);
        for r in 0..rounds {
            let b = &encoded[r % encoded.len()];
            let t0 = SubMsTimer::tick();
            let d = doc_from_bytes(b).expect("decode");
            let (ts, _v) = point_from_doc(&d).expect("point");
            s_decode.record(t0.elapsed_ns());
            std::hint::black_box(ts);
        }

        h.add_meta("points", &POINTS.to_string());
        h.add_meta("subms.workload.feature", "mongodb");
    }
}
