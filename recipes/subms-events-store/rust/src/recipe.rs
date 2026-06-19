//! `SubMsRecipe` impl. Stages: `append` (push + notify), `replay` (full fold over
//! a fixed log), `catch_up` (incremental projection of the tail).

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::{Event, EventStore, Projector, replay};

pub struct EventStoreRecipe;

const REPLAY_N: usize = 1_000;

impl SubMsRecipe for EventStoreRecipe {
    fn name(&self) -> &str {
        "subms-events-store"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;

        // append: into a fresh store.
        let mut store = EventStore::new();
        let s_app = h
            .stage("append", entries)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..entries {
            let ev = Event::builder("evt")
                .at("t")
                .attr("i", &i.to_string())
                .build();
            let t0 = SubMsTimer::tick();
            store.append(ev);
            s_app.record(t0.elapsed_ns());
        }

        // replay: full fold over a fixed REPLAY_N-event store.
        let mut base = EventStore::new();
        for _ in 0..REPLAY_N {
            base.append(Event::builder("evt").at("t").build());
        }
        let s_rep = h
            .stage("replay", entries)
            .with_kind(SubMsStageKind::BatchOp);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let count = replay(&base, 0u64, |n, _e| *n += 1);
            s_rep.record(t0.elapsed_ns());
            std::hint::black_box(count);
        }

        // catch_up: incremental projection of one new event each round.
        let mut proj = Projector::new(0u64);
        proj.catch_up(&base, |n, _e| *n += 1);
        let s_cu = h
            .stage("catch_up", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            base.append(Event::builder("x").at("t").build());
            let t0 = SubMsTimer::tick();
            proj.catch_up(&base, |n, _e| *n += 1);
            s_cu.record(t0.elapsed_ns());
        }

        h.add_meta("replay_window", &REPLAY_N.to_string());
        h.add_meta("final_log_len", &base.len().to_string());
        h.add_meta("subms.workload.feature", "in-memory-event-sourcing");
    }
}
