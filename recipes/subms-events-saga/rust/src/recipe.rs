//! `SubMsRecipe` impl. Stages: `build` (assemble an 8-step saga), `commit` (run a
//! saga where every forward succeeds), `compensate` (run one where the last
//! forward fails, rolling back the rest).

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::Saga;

pub struct SagaRecipe;

const STEPS: usize = 8;

fn commit_saga() -> Saga {
    let mut s = Saga::new("bench");
    for i in 0..STEPS {
        s = s.step(&format!("s{i}"), || Ok(()), || Ok(()));
    }
    s
}

fn fail_saga() -> Saga {
    let mut s = Saga::new("bench");
    for i in 0..(STEPS - 1) {
        s = s.step(&format!("s{i}"), || Ok(()), || Ok(()));
    }
    s.step("last", || Err("boom".to_string()), || Ok(()))
}

impl SubMsRecipe for SagaRecipe {
    fn name(&self) -> &str {
        "subms-events-saga"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;

        let s_build = h.stage("build", entries).with_kind(SubMsStageKind::BatchOp);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let saga = commit_saga();
            s_build.record(t0.elapsed_ns());
            std::hint::black_box(&saga);
        }

        let saga = commit_saga();
        let s_commit = h
            .stage("commit", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let r = saga.run();
            s_commit.record(t0.elapsed_ns());
            std::hint::black_box(r.is_committed());
        }

        let fsaga = fail_saga();
        let s_comp = h
            .stage("compensate", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let r = fsaga.run();
            s_comp.record(t0.elapsed_ns());
            std::hint::black_box(r.compensated.len());
        }

        h.add_meta("steps", &STEPS.to_string());
        h.add_meta("subms.workload.feature", "in-process-compensation");
    }
}
