//! `SubMsRecipe` impl. Stages: `build` (construct an event via the builder),
//! `emit_sync` (inline dispatch to a listener), `emit_async` (enqueue to the
//! dispatcher thread).

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind, SubMsTimer};

use crate::{Event, EventDispatcher, EventLevel, listener};

pub struct EventsRecipe;

impl SubMsRecipe for EventsRecipe {
    fn name(&self) -> &str {
        "subms-events"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let entries = params.entries;

        let s_build = h.stage("build", entries).with_kind(SubMsStageKind::BatchOp);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            let e = Event::transition("svc.status", EventLevel::Error, "db", "UP", "DOWN");
            s_build.record(t0.elapsed_ns());
            std::hint::black_box(e.topic.len());
        }

        let ev = Event::transition("svc.status", EventLevel::Error, "db", "UP", "DOWN");

        let sync_count = Arc::new(AtomicU64::new(0));
        let sc = Arc::clone(&sync_count);
        let mut sync_bus = EventDispatcher::sync();
        sync_bus.add_listener(listener(move |_e| {
            sc.fetch_add(1, Ordering::Relaxed);
        }));
        let s_sync = h
            .stage("emit_sync", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            sync_bus.emit(ev.clone());
            s_sync.record(t0.elapsed_ns());
        }

        let async_count = Arc::new(AtomicU64::new(0));
        let ac = Arc::clone(&async_count);
        let mut async_bus = EventDispatcher::asynchronous();
        async_bus.add_listener(listener(move |_e| {
            ac.fetch_add(1, Ordering::Relaxed);
        }));
        let s_async = h
            .stage("emit_async", entries)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..entries {
            let t0 = SubMsTimer::tick();
            async_bus.emit(ev.clone());
            s_async.record(t0.elapsed_ns());
        }
        async_bus.stop();

        h.add_meta("subms.workload.feature", "in-process-dispatch");
        h.add_meta(
            "sync_delivered",
            &sync_count.load(Ordering::Relaxed).to_string(),
        );
    }
}
