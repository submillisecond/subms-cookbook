//! `SubMsRecipe` impl - push (streaming ingest), query (read the rolling
//! aggregates), and merge (fold two partition windows).

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsWindowedAggregator;

pub struct AggregatorRecipe;

const WINDOW_NS: i64 = 1_024;
const MERGE_ROUNDS: usize = 2_000;

impl SubMsRecipe for AggregatorRecipe {
    fn name(&self) -> &str {
        "subms-ts-aggregator"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;

        // ---------- push: streaming ingest, window holds ~1024 points ----------
        let mut agg = TsWindowedAggregator::new(WINDOW_NS);
        let s_push = h.stage("push", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed);
        for i in 0..entries {
            let v = (rng.next_u32() >> 8) as f64 / 65_536.0;
            let t0 = SubMsTimer::tick();
            agg.push(i as i64, v);
            s_push.record(t0.elapsed_ns());
        }

        // ---------- query: O(1) reads of the rolling aggregates ----------
        let s_q = h.stage("query", entries).with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let r = (
                agg.min().unwrap_or(0.0),
                agg.max().unwrap_or(0.0),
                agg.sum(),
                agg.mean().unwrap_or(0.0),
            );
            s_q.record(t0.elapsed_ns());
            std::hint::black_box(r);
        }

        // ---------- merge: fold two partition windows ----------
        let mut left = TsWindowedAggregator::new(WINDOW_NS);
        let mut right = TsWindowedAggregator::new(WINDOW_NS);
        let mut rng2 = SubMsLcg::new(seed ^ 0x55);
        for i in 0..WINDOW_NS {
            left.push(i, (rng2.next_u32() >> 16) as f64);
            right.push(i, (rng2.next_u32() >> 16) as f64);
        }
        let s_m = h
            .stage("merge", MERGE_ROUNDS)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..MERGE_ROUNDS {
            let t0 = SubMsTimer::tick();
            let m = left.merge(&right);
            s_m.record(t0.elapsed_ns());
            std::hint::black_box(m.count());
        }

        h.add_meta("window_ns", &WINDOW_NS.to_string());
        h.add_meta("subms.workload.feature", "rolling-window");
    }
}
