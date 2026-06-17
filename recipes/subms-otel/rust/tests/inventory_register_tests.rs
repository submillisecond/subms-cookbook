//! Verify `register_observer` lands an observer in `registered_observers()`
//! and `auto_configure()` picks it up ahead of the default builder.

#![cfg(feature = "autoconfig")]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};

use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver};
use subms_otel::{
    auto_configure, clear_registered_observers, register_observer, registered_observers,
};

static TOUCHED: AtomicBool = AtomicBool::new(false);

fn registry_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

struct MarkerObserver;

impl SubMsObserver for MarkerObserver {
    fn on_record(&self, _ctx: &ObservationCtx, _ns: u64) {
        TOUCHED.store(true, Ordering::SeqCst);
    }
    fn on_summarize(&self, _summary: &SubMsBenchSummary) {}
}

#[test]
fn registered_observers_lists_registered() {
    let _g = registry_lock().lock().unwrap();
    clear_registered_observers();
    register_observer("marker", Arc::new(MarkerObserver));
    let registered = registered_observers();
    assert!(
        !registered.is_empty(),
        "register_observer should surface the marker registration"
    );
    clear_registered_observers();
}

#[test]
fn auto_configure_uses_registered_marker() {
    let _g = registry_lock().lock().unwrap();
    clear_registered_observers();
    register_observer("marker", Arc::new(MarkerObserver));
    TOUCHED.store(false, Ordering::SeqCst);
    let cfg = auto_configure();
    let ctx = ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: subms::SubMsStageKind::HotPath,
    };
    cfg.observer.on_record(&ctx, 1);
    assert!(
        TOUCHED.load(Ordering::SeqCst),
        "auto_configure should pick the registered observer"
    );
    clear_registered_observers();
}
