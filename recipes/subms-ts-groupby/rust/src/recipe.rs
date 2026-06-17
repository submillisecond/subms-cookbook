//! `SubMsRecipe` impl - a representative group-by-aggregate workload. The frame
//! is 4,096 rows keyed by a low-cardinality `venue` STRING (8 distinct symbols),
//! with `size` and `price` f64 columns. Two stages: `group_agg` runs the full
//! partition + three aggregations (sum, mean, count) per group; `value_counts`
//! runs the single-column count path. Throughput-contracted: each timed sample
//! is a full group-by over the whole frame, not a single op.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};
use subms_ts_expr::TsExpr;

use crate::{group_by, value_counts};

pub struct GroupByRecipe;

const ROWS: usize = 4_096;
const CARDINALITY: u32 = 8;

const VENUES: [&str; 8] = [
    "ARCA", "BATS", "EDGX", "IEX", "NSDQ", "NYSE", "PHLX", "XCBO",
];

fn build_frame(seed: u64) -> TsDataFrame {
    let mut rng = SubMsLcg::new(seed);
    let mut venue = TsSeries::<String>::with_capacity(ROWS);
    let mut size = TsSeries::<f64>::with_capacity(ROWS);
    let mut price = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let v = VENUES[(rng.next_u32() % CARDINALITY) as usize];
        let s = ((rng.next_u32() >> 18) as f64) + 1.0;
        let p = ((rng.next_u32() >> 16) as f64) / 100.0;
        let _ = venue.push(i as i64, v.to_string());
        let _ = size.push(i as i64, s);
        let _ = price.push(i as i64, p);
    }
    TsDataFrame::new()
        .with_column("venue", TsColumn::Str(venue))
        .with_column("size", TsColumn::F64(size))
        .with_column("price", TsColumn::F64(price))
}

impl SubMsRecipe for GroupByRecipe {
    fn name(&self) -> &str {
        "subms-ts-groupby"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let frame = build_frame(params.seed);

        let s_agg = h
            .stage("group_agg", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let result = group_by(&frame, &["venue"])
                .unwrap()
                .agg(&[
                    ("total_size", TsExpr::col("size").sum()),
                    ("mean_price", TsExpr::col("price").mean()),
                    ("n", TsExpr::col("size").count()),
                ])
                .unwrap();
            s_agg.record(t0.elapsed_ns());
            std::hint::black_box(result.nrows());
        }

        let s_vc = h
            .stage("value_counts", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let vc = value_counts(&frame, "venue").unwrap();
            s_vc.record(t0.elapsed_ns());
            std::hint::black_box(vc.nrows());
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("key_cardinality", &CARDINALITY.to_string());
        h.add_meta("key_type", "str");
        h.add_meta("subms.workload.feature", "group-by-aggregate");
    }
}
