//! Tests for `CompositeObserver`: every inner observer should see every
//! sample and the post-bench summary.

#![cfg(feature = "bridge")]

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver, SubMsStageKind};
use subms_otel::CompositeObserver;

#[derive(Default)]
struct Counter {
    records: AtomicU64,
    summaries: AtomicU64,
}

impl SubMsObserver for Counter {
    fn on_record(&self, _ctx: &ObservationCtx, _ns: u64) {
        self.records.fetch_add(1, Ordering::Relaxed);
    }
    fn on_summarize(&self, _summary: &SubMsBenchSummary) {
        self.summaries.fetch_add(1, Ordering::Relaxed);
    }
}

fn fake_ctx<'a>() -> ObservationCtx<'a> {
    ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: SubMsStageKind::HotPath,
    }
}

fn fake_summary() -> SubMsBenchSummary {
    SubMsBenchSummary {
        workload: "wl".into(),
        lang: "rust".into(),
        timestamp: "2026-05-30T00:00:00Z".into(),
        cpu_core: None,
        cpu_affinity: None,
        inputs: Default::default(),
        meta: Default::default(),
        stages: vec![],
    }
}

#[test]
fn fans_out_to_every_inner_observer() {
    let a = Arc::new(Counter::default());
    let b = Arc::new(Counter::default());
    let composite = CompositeObserver::new(vec![
        Arc::clone(&a) as Arc<dyn SubMsObserver>,
        Arc::clone(&b) as Arc<dyn SubMsObserver>,
    ]);

    let ctx = fake_ctx();
    for ns in 1..=10u64 {
        composite.on_record(&ctx, ns);
    }
    composite.on_summarize(&fake_summary());

    assert_eq!(a.records.load(Ordering::Relaxed), 10);
    assert_eq!(b.records.load(Ordering::Relaxed), 10);
    assert_eq!(a.summaries.load(Ordering::Relaxed), 1);
    assert_eq!(b.summaries.load(Ordering::Relaxed), 1);
}

#[test]
fn empty_composite_is_a_noop() {
    let composite = CompositeObserver::new(vec![]);
    let ctx = fake_ctx();
    composite.on_record(&ctx, 100);
    composite.on_summarize(&fake_summary());
    assert!(composite.observers().is_empty());
}

#[test]
fn builder_with_appends_observers() {
    let a = Arc::new(Counter::default());
    let b = Arc::new(Counter::default());
    let composite = CompositeObserver::new(vec![Arc::clone(&a) as Arc<dyn SubMsObserver>])
        .with(Arc::clone(&b) as Arc<dyn SubMsObserver>);

    assert_eq!(composite.observers().len(), 2);
    composite.on_record(&fake_ctx(), 42);
    assert_eq!(a.records.load(Ordering::Relaxed), 1);
    assert_eq!(b.records.load(Ordering::Relaxed), 1);
}
