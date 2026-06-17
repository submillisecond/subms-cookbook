//! `SubMsRecipe` impl - two stages. `parse` repeatedly parses a moderate
//! query (the cost of the lexer + recursive-descent walk in isolation).
//! `eval` parses once and evaluates `sum by (job) (rate(metric[5m]))` over a
//! collection of a few hundred tagged counter series (the resolution + range
//! scan + grouping cost).

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};
use subms_ts::{TsCollection, TsSeriesMetadata};

use crate::TsPromQl;

pub struct PromQlRecipe;

const SERIES: u64 = 200;
const POINTS: i64 = 32;
const STEP_NS: i64 = 15_000_000_000; // 15s scrape interval
const JOBS: u64 = 6;

const PARSE_QUERY: &str = "sum by (job) (rate(http_requests_total{job=~\"api.*\", code!=\"500\"}[5m])) / count by (job) (http_requests_total{job=~\"api.*\"})";
const EVAL_QUERY: &str = "sum by (job) (rate(http_requests_total[5m]))";

fn build_collection() -> TsCollection<f64> {
    let mut coll = TsCollection::<f64>::new();
    for i in 0..SERIES {
        let meta = TsSeriesMetadata::new(i, "")
            .with_tag("__name__", "http_requests_total")
            .with_tag("job", format!("api-{}", i % JOBS))
            .with_tag("instance", format!("host-{i}"))
            .with_tag("code", "200");
        coll.register(meta).expect("register");
        // monotone counter: each series climbs at its own slope.
        let slope = 1.0 + (i % 7) as f64;
        let mut acc = 0.0;
        for p in 0..POINTS {
            acc += slope;
            coll.push(i, p * STEP_NS, acc).expect("push");
        }
    }
    coll
}

impl SubMsRecipe for PromQlRecipe {
    fn name(&self) -> &str {
        "subms-ts-promql"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;
        let mut rng = SubMsLcg::new(params.seed);

        let s_parse = h.stage("parse", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let expr = crate::parser::parse(PARSE_QUERY).expect("parse");
            s_parse.record(t0.elapsed_ns());
            std::hint::black_box(&expr);
            std::hint::black_box(rng.next_u32());
        }

        let coll = build_collection();
        let at = (POINTS - 1) * STEP_NS;
        let engine = TsPromQl::new(&coll);
        let s_eval = h.stage("eval", rounds).with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let res = engine.eval_instant(EVAL_QUERY, at).expect("eval");
            s_eval.record(t0.elapsed_ns());
            std::hint::black_box(res.len());
        }

        h.add_meta("series", &SERIES.to_string());
        h.add_meta("points_per_series", &POINTS.to_string());
        h.add_meta("subms.workload.feature", "promql");
    }
}
