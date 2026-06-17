//! `SubMsRecipe` impl - drive two stages. `optimise_collect` builds a 5-op lazy
//! pipeline over a 4,096-row frame, optimises it, and collects the result -
//! throughput-contracted (a generous guard, not a tight p99; collecting a whole
//! frame is the analytical front, not the tick loop). `certify` lowers the same
//! pipeline to a latency certificate - per-op work over the plan node list,
//! independent of row count, and genuinely sub-ms.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};
use subms_ts_expr::TsExpr;

use crate::LazyTsFrame;

pub struct LazyRecipe;

const ROWS: usize = 4_096;

fn build_frame(seed: u64) -> TsDataFrame {
    let mut rng = SubMsLcg::new(seed);
    let mut px = TsSeries::<f64>::with_capacity(ROWS);
    let mut qty = TsSeries::<i64>::with_capacity(ROWS);
    let mut venue = TsSeries::<i64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let p = ((rng.next_u32() >> 16) as f64) / 64.0;
        let q = (rng.next_u32() >> 24) as i64;
        let v = (rng.next_u32() % 4) as i64;
        let _ = px.push(i as i64, p);
        let _ = qty.push(i as i64, q);
        let _ = venue.push(i as i64, v);
    }
    TsDataFrame::new()
        .with_column("px", TsColumn::F64(px))
        .with_column("qty", TsColumn::I64(qty))
        .with_column("venue", TsColumn::I64(venue))
}

// A 5-op pipeline: filter -> with_column -> filter -> sort -> select. The kind
// of shape a real analytical query lowers to.
fn pipeline(frame: TsDataFrame) -> LazyTsFrame {
    LazyTsFrame::new(frame)
        .filter(TsExpr::col("px").gt(TsExpr::lit_f64(128.0)))
        .with_column("notional", TsExpr::col("px").mul(TsExpr::col("qty")))
        .filter(TsExpr::col("venue").eq(TsExpr::lit_i64(1)))
        .sort_by("notional", false)
        .select(&["notional", "px"])
}

impl SubMsRecipe for LazyRecipe {
    fn name(&self) -> &str {
        "subms-ts-lazy"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let frame = build_frame(params.seed);

        let s_collect = h
            .stage("optimise_collect", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let result = pipeline(clone_frame(&frame)).collect().unwrap();
            s_collect.record(t0.elapsed_ns());
            std::hint::black_box(result.nrows());
        }

        let s_certify = h
            .stage("certify", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let lazy = pipeline(clone_frame(&frame));
            let t0 = SubMsTimer::tick();
            let cert = lazy.certify("ci-dedicated", 0);
            s_certify.record(t0.elapsed_ns());
            std::hint::black_box(cert.total_p99_ns);
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("subms.workload.feature", "lazy-plan-certify");
    }
}

// The frame has no Clone, so rebuild it per round from its columns. Cheap
// relative to the pipeline work and keeps each round independent.
fn clone_frame(frame: &TsDataFrame) -> TsDataFrame {
    let mut out = TsDataFrame::new();
    for name in frame
        .column_names()
        .map(|s| s.to_string())
        .collect::<Vec<_>>()
    {
        if let Some(col) = frame.column(&name) {
            out.push_column(name, clone_column(col)).unwrap();
        }
    }
    out
}

fn clone_column(col: &TsColumn) -> TsColumn {
    match col {
        TsColumn::F64(s) => TsColumn::F64(s.clone()),
        TsColumn::I64(s) => TsColumn::I64(s.clone()),
        TsColumn::Bool(s) => TsColumn::Bool(s.clone()),
        TsColumn::Str(s) => TsColumn::Str(s.clone()),
        TsColumn::Value(s) => TsColumn::Value(s.clone()),
    }
}
