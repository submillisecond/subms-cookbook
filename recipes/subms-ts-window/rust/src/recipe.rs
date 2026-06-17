//! `SubMsRecipe` impl - run three representative window passes over a
//! STRING-partitioned 4,096-row [`TsDataFrame`]: `lag` (shift), `cumsum`
//! (running reduction), and `over` (per-partition aggregate broadcast). The
//! frame is partitioned by a low-cardinality `symbol` STRING (16 venues) so
//! each pass does a realistic typed-key group-sort-scan, not a degenerate
//! single-partition or all-singleton case. Throughput-contracted: each timed
//! sample is a full whole-frame window pass, and `over` (per-partition
//! sub-frame + expr eval) is the heavy stage.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};
use subms_ts_expr::TsExpr;

use crate::{cumsum, lag, over};

pub struct WindowRecipe;

const ROWS: usize = 4_096;
const PARTITIONS: u32 = 16;

const SYMBOLS: [&str; 16] = [
    "ARCA", "BATS", "EDGX", "IEX", "NSDQ", "NYSE", "PHLX", "XCBO", "LSE", "TSE", "HKEX", "SGX",
    "ASX", "BMV", "JSE", "B3",
];

fn build_frame(seed: u64) -> TsDataFrame {
    let mut rng = SubMsLcg::new(seed);
    let mut symbol = TsSeries::<String>::with_capacity(ROWS);
    let mut val = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let s = SYMBOLS[(rng.next_u32() % PARTITIONS) as usize];
        let v = (rng.next_u32() >> 16) as f64;
        let _ = symbol.push(i as i64, s.to_string());
        let _ = val.push(i as i64, v);
    }
    TsDataFrame::new()
        .with_column("symbol", TsColumn::Str(symbol))
        .with_column("val", TsColumn::F64(val))
}

impl SubMsRecipe for WindowRecipe {
    fn name(&self) -> &str {
        "subms-ts-window"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let frame = build_frame(params.seed);
        let keys = ["symbol"];
        let agg = TsExpr::col("val").mean();

        let s_lag = h.stage("lag", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let arr = lag(&frame, "val", 1, &keys).unwrap();
            s_lag.record(t0.elapsed_ns());
            std::hint::black_box(arr.len());
        }

        let s_cum = h.stage("cumsum", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let arr = cumsum(&frame, "val", &keys, None).unwrap();
            s_cum.record(t0.elapsed_ns());
            std::hint::black_box(arr.len());
        }

        let s_over = h.stage("over", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let arr = over(&frame, &agg, &keys).unwrap();
            s_over.record(t0.elapsed_ns());
            std::hint::black_box(arr.len());
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("partitions", &PARTITIONS.to_string());
        h.add_meta("key_type", "str");
        h.add_meta("subms.workload.feature", "window-functions");
    }
}
