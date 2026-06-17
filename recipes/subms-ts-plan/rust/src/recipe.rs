//! `SubMsRecipe` impl - build a representative query plan + certify it.

use subms::{
    SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer,
};

use crate::TsPlan;

pub struct PlanRecipe;

impl SubMsRecipe for PlanRecipe {
    fn name(&self) -> &str {
        "subms-ts-plan"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let rounds = params.entries;

        // a 10-stage plan, the shape a real prune->decode->scan->quantile query
        // lowers to.
        let build = || {
            let mut p = TsPlan::new().with_overhead(50_000);
            for i in 0..10 {
                p = p.then("subms-ts", "range_min", 900 + i as u64 * 10);
            }
            p
        };

        let s_cert = h
            .stage("certify", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let plan = build();
            let t0 = SubMsTimer::tick();
            let cert = plan.certify("ci-dedicated", 0);
            s_cert.record(t0.elapsed_ns());
            std::hint::black_box(cert.total_p99_ns);
        }

        let cert = build().certify("ci-dedicated", 0);
        let s_verify = h
            .stage("verify", rounds)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..rounds {
            let t0 = SubMsTimer::tick();
            let ok = cert.verify();
            s_verify.record(t0.elapsed_ns());
            std::hint::black_box(ok);
        }

        h.add_meta("plan_stages", "10");
        h.add_meta("subms.workload.feature", "latency-certificate");
    }
}
