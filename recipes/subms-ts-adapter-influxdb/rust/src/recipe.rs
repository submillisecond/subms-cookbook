//! `SubMsRecipe` impl - two pure stages. `encode` builds the line-protocol
//! batch for a tagged series; `decode` parses an annotated-CSV Flux response
//! back into a collection. Both are the measurable hot path; the network round
//! trip is not benched (it is network-bound and reported, not claimed). The
//! contract is throughput, so the bench asserts a generous no-stall guard.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsSeries, TsSeriesMetadata};

use crate::{decode_response, encode_series, format_rfc3339_nanos};

pub struct InfluxRecipe;

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

fn build_csv(series: &TsSeries<f64>) -> String {
    let mut out = String::from(
        "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n\
         ,result,table,_time,_value,_field,_measurement,host\n",
    );
    for p in series.iter() {
        out.push_str(",_result,0,");
        out.push_str(&format_rfc3339_nanos(p.ts));
        out.push(',');
        out.push_str(&p.value.to_string());
        out.push_str(",v,cpu,edge-01\n");
    }
    out
}

impl SubMsRecipe for InfluxRecipe {
    fn name(&self) -> &str {
        "subms-ts-adapter-influxdb"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let series = build_series(params.seed);

        let s_encode = h.stage("encode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let body = encode_series(&series, "");
            s_encode.record(t0.elapsed_ns());
            std::hint::black_box(body.len());
        }

        let csv = build_csv(&series);
        let s_decode = h.stage("decode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let coll = decode_response(&csv).expect("decode");
            s_decode.record(t0.elapsed_ns());
            std::hint::black_box(coll.len());
        }

        h.add_meta("points", &POINTS.to_string());
        h.add_meta("subms.workload.feature", "influxdb");
    }
}
