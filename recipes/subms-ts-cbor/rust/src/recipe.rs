//! `SubMsRecipe` impl - CBOR encode + decode round over a representative
//! `f64` series.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsCodec, TsSeries};

use crate::TsCborCodec;

pub struct CborRecipe;

const SERIES: usize = 1_024;

impl SubMsRecipe for CborRecipe {
    fn name(&self) -> &str {
        "subms-ts-cbor"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let seed = params.seed;

        let mut rng = SubMsLcg::new(seed);
        let mut s = TsSeries::<f64>::with_capacity(SERIES);
        let mut ts = 0i64;
        for _ in 0..SERIES {
            let _ = s.push(ts, (rng.next_u32() >> 16) as f64 / 7.0);
            ts += 10 + (rng.next_u32() % 40) as i64;
        }
        let codec = TsCborCodec::new();
        let encoded = codec.encode(&s);

        let s_enc = h.stage("encode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let bytes = codec.encode(&s);
            s_enc.record(t0.elapsed_ns());
            std::hint::black_box(bytes.len());
        }

        let s_dec = h.stage("decode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let back = codec.decode(&encoded).unwrap();
            s_dec.record(t0.elapsed_ns());
            std::hint::black_box(back.len());
        }

        h.add_meta("series_points", &SERIES.to_string());
        h.add_meta("encoded_bytes", &encoded.len().to_string());
        h.add_meta("subms.workload.feature", "cbor-codec");
    }
}
