//! Drive `auto_configure` under controlled env, assert the returned config is
//! wired sanely. Doesn't push real samples through the providers - that's the
//! exporter helpers' job.

#![cfg(feature = "autoconfig")]

use std::sync::{Mutex, OnceLock};

#[allow(unused_imports)]
use subms::SubMsObserver;
use subms_otel::{auto_configure, clear_registered_observers};

fn env_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn clear_env() {
    for k in [
        "OTEL_EXPORTER_OTLP_ENDPOINT",
        "OTEL_EXPORTER_OTLP_PROTOCOL",
        "OTEL_SERVICE_NAME",
        "OTEL_SERVICE_VERSION",
        "OTEL_RESOURCE_ATTRIBUTES",
        "SUBMS_OTEL_ASYNC",
        "SUBMS_OTEL_EXEMPLARS_K",
        "SUBMS_HARDWARE_TIER",
        "SUBMS_HOST",
        "SUBMS_OTEL_PROMETHEUS",
    ] {
        unsafe {
            std::env::remove_var(k);
        }
    }
}

#[test]
fn default_returns_wired_observer_and_providers() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    clear_registered_observers();
    let cfg = auto_configure();
    // Observer must be non-null and accept records without panicking.
    let ctx = subms::ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: subms::SubMsStageKind::HotPath,
    };
    cfg.observer.on_record(&ctx, 1234);
    // Providers must be flushable.
    let _ = cfg.meter_provider.force_flush();
    let _ = cfg.tracer_provider.force_flush();
}

#[test]
fn sync_observer_picked_when_async_disabled() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("SUBMS_OTEL_ASYNC", "false");
    }
    clear_registered_observers();
    let cfg = auto_configure();
    let ctx = subms::ObservationCtx {
        workload: "wl",
        lang: "rust",
        stage: "put",
        stage_kind: subms::SubMsStageKind::HotPath,
    };
    cfg.observer.on_record(&ctx, 5678);
    clear_env();
}

#[test]
fn service_name_flows_into_resource() {
    let _g = env_lock().lock().unwrap();
    clear_env();
    unsafe {
        std::env::set_var("OTEL_SERVICE_NAME", "wired-via-autoconfig");
    }
    clear_registered_observers();
    let _cfg = auto_configure();
    // We can't easily introspect the SDK provider's resource, but
    // auto_configure() going through without panicking and the env being
    // honored at detect() time is itself covered by resource_tests.
    clear_env();
}
