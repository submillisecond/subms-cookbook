//! `SubMsRecipe` impl - streaming push with z-score scoring + expiry.

use subms::{
    SubMsBenchParams, SubMsLcg, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsAnomalyDetector;

pub struct AnomalyRecipe;

const WINDOW_NS: i64 = 1_024;
const SIGMA: f64 = 3.0;

impl SubMsRecipe for AnomalyRecipe {
    fn name(&self) -> &str {
        "subms-ts-anomaly"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;
        let seed = params.seed;

        let mut d = TsAnomalyDetector::new(WINDOW_NS, SIGMA);
        let s_push = h.stage("push", entries).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(seed);
        let mut flagged = 0u64;
        for i in 0..entries as i64 {
            // baseline ~100 with jitter; occasional spike
            let base = 100.0 + (rng.next_u32() >> 24) as f64 / 256.0;
            let v = if rng.next_u32() % 5_000 == 0 {
                base + 500.0
            } else {
                base
            };
            let t0 = SubMsTimer::tick();
            let hit = d.push(i, v);
            s_push.record(t0.elapsed_ns());
            if hit.is_some() {
                flagged += 1;
            }
        }

        h.add_meta("flagged", &flagged.to_string());
        h.add_meta("subms.workload.feature", "rolling-zscore");
    }
}
