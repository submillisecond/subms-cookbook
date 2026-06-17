//! `SubMsRecipe` impl. `encode` persists a series to Parquet bytes; `decode`
//! reads it back. Parquet does more than a raw columnar copy (row groups, column
//! chunks, page headers, a thrift footer with statistics), so the asserted
//! workload is a modest series where the whole encode / decode still clears
//! sub-ms. Larger files are reported throughput, not asserted.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsSeries, TsSeriesMetadata};

use crate::{parquet_to_series, series_to_parquet};

pub struct ParquetRecipe;

const POINTS: usize = 256;

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

impl SubMsRecipe for ParquetRecipe {
    fn name(&self) -> &str {
        "subms-ts-adapter-parquet"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let series = build_series(params.seed);

        let s_enc = h.stage("encode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let bytes = series_to_parquet(&series).expect("encode");
            s_enc.record(t0.elapsed_ns());
            std::hint::black_box(bytes.len());
        }

        let bytes = series_to_parquet(&series).expect("encode");
        let s_dec = h.stage("decode", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let s = parquet_to_series(&bytes).expect("decode");
            s_dec.record(t0.elapsed_ns());
            std::hint::black_box(s.len());
        }

        h.add_meta("points", &POINTS.to_string());
        h.add_meta("subms.workload.feature", "parquet");
    }
}
