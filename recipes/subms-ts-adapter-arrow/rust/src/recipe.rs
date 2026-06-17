//! `SubMsRecipe` impl. `to_batch` builds an Arrow `RecordBatch` from a whole
//! series; `from_batch` reads it back. Unlike a per-document codec, the columnar
//! build is two bulk buffer fills, so the whole-series convert is the per-op
//! primitive and the asserted sub-ms claim. IPC framing is reported separately,
//! not asserted (it is bulk + impl-defined).

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsSeries, TsSeriesMetadata};

use crate::{batch_to_series, series_to_batch};

pub struct ArrowRecipe;

const POINTS: usize = 1_024;

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

impl SubMsRecipe for ArrowRecipe {
    fn name(&self) -> &str {
        "subms-ts-adapter-arrow"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let series = build_series(params.seed);

        let s_to = h
            .stage("to_batch", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let batch = series_to_batch(&series).expect("to_batch");
            s_to.record(t0.elapsed_ns());
            std::hint::black_box(batch.num_rows());
        }

        let batch = series_to_batch(&series).expect("to_batch");
        let s_from = h
            .stage("from_batch", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let s = batch_to_series(&batch).expect("from_batch");
            s_from.record(t0.elapsed_ns());
            std::hint::black_box(s.len());
        }

        h.add_meta("points", &POINTS.to_string());
        h.add_meta("subms.workload.feature", "arrow");
    }
}
