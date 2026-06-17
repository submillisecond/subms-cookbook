//! `SubMsRecipe` impl - evaluate a representative multi-node expression over a
//! frame. Two stages: `eval_pipeline` evaluates a per-row tree (a `When` over a
//! `Compare` with two `Binary` arms) to a full array; `eval_agg` reduces that
//! same tree to a scalar mean. Throughput-contracted: each timed sample is a
//! full evaluation over a 4,096-row frame, not a single op.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsColumn, TsDataFrame, TsSeries};

use crate::{TsExpr, eval, eval_scalar, when};

pub struct ExprRecipe;

const ROWS: usize = 4_096;

fn build_frame(seed: u64) -> TsDataFrame {
    let mut rng = SubMsLcg::new(seed);
    let mut open = TsSeries::<f64>::with_capacity(ROWS);
    let mut close = TsSeries::<f64>::with_capacity(ROWS);
    for i in 0..ROWS {
        let o = (rng.next_u32() >> 16) as f64;
        let c = (rng.next_u32() >> 16) as f64;
        let _ = open.push(i as i64, o);
        let _ = close.push(i as i64, c);
    }
    TsDataFrame::new()
        .with_column("open", TsColumn::F64(open))
        .with_column("close", TsColumn::F64(close))
}

fn pipeline() -> TsExpr {
    // when(close > open, close - open, 0.0).
    when(
        TsExpr::col("close").gt(TsExpr::col("open")),
        TsExpr::col("close").sub(TsExpr::col("open")),
        TsExpr::lit_f64(0.0),
    )
}

impl SubMsRecipe for ExprRecipe {
    fn name(&self) -> &str {
        "subms-ts-expr"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let frame = build_frame(params.seed);
        let per_row = pipeline();
        let reduced = pipeline().mean();

        let s_pipe = h
            .stage("eval_pipeline", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let arr = eval(&per_row, &frame).unwrap();
            s_pipe.record(t0.elapsed_ns());
            std::hint::black_box(arr.len());
        }

        let s_agg = h
            .stage("eval_agg", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let v = eval_scalar(&reduced, &frame).unwrap();
            s_agg.record(t0.elapsed_ns());
            std::hint::black_box(&v);
        }

        h.add_meta("frame_rows", &ROWS.to_string());
        h.add_meta("subms.workload.feature", "expr-eval");
    }
}
