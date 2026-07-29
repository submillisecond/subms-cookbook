//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! worst-wins rollup, probe-aware codes, non-critical demotion, and redaction on
//! the base path; the timestamp shape behind `datetime`; the status-change
//! fan-out behind `otel`.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::*;

fn gateway(session_up: &Arc<AtomicBool>, risk_tight: &Arc<AtomicBool>) -> HealthRegistry {
    let mut reg = HealthRegistry::with_config(HealthConfig::sync());
    reg.register_fn(
        "market-data-feed",
        RefreshPolicy::new().with_kinds(&[ProbeKind::Readiness]),
        ComponentHealth::up,
    );
    {
        let risk_tight = Arc::clone(risk_tight);
        reg.register_fn("risk-check", RefreshPolicy::new(), move || {
            if risk_tight.load(Ordering::Relaxed) {
                ComponentHealth::degraded("pre-trade limit 92% utilised")
            } else {
                ComponentHealth::up()
            }
        });
    }
    {
        let session_up = Arc::clone(session_up);
        reg.register_fn(
            "exchange-session",
            RefreshPolicy::new().with_kinds(&[ProbeKind::Liveness, ProbeKind::Readiness]),
            move || {
                if session_up.load(Ordering::Relaxed) {
                    ComponentHealth::up()
                } else {
                    ComponentHealth::down("gateway logged out")
                }
            },
        );
    }
    reg.register_fn(
        "orderbook-cache",
        RefreshPolicy::new().critical(false),
        || ComponentHealth::down("warm cache miss"),
    );
    reg.start();
    reg
}

#[test]
fn non_critical_failure_is_demoted() {
    let session_up = Arc::new(AtomicBool::new(true));
    let risk_tight = Arc::new(AtomicBool::new(false));
    let reg = gateway(&session_up, &risk_tight);

    let (code, _) = reg.render();
    assert_eq!(reg.status(), HealthStatus::Warn, "cache demoted to WARN");
    assert_eq!(code, 200, "still serving");
}

#[test]
fn degraded_is_probe_aware() {
    let session_up = Arc::new(AtomicBool::new(true));
    let risk_tight = Arc::new(AtomicBool::new(false));
    let reg = gateway(&session_up, &risk_tight);

    risk_tight.store(true, Ordering::Relaxed);
    reg.refresh_now();

    assert_eq!(reg.status(), HealthStatus::Degraded, "worst-wins over WARN");
    assert_eq!(reg.render_readiness().0, 503, "pulled from rotation");
    assert_eq!(reg.render_liveness().0, 200, "not restarted");
}

#[test]
fn down_wins_and_restarts() {
    let session_up = Arc::new(AtomicBool::new(true));
    let risk_tight = Arc::new(AtomicBool::new(true));
    let reg = gateway(&session_up, &risk_tight);

    session_up.store(false, Ordering::Relaxed);
    reg.refresh_now();

    assert_eq!(reg.status(), HealthStatus::Down, "DOWN beats DEGRADED");
    assert_eq!(reg.render_liveness().0, 503, "hard down restarts the pod");
}

#[test]
fn deploy_section_masks_secrets() {
    let mut reg = HealthRegistry::with_config(HealthConfig::sync());
    let env = MapEnv::new()
        .with("KICKSTART_REGION", "eu-west-1")
        .with("KICKSTART_API_TOKEN", "live-abc123-do-not-log");
    let deploy = EnvSection::new("deploy")
        .prefix("KICKSTART_")
        .strip_prefix_in_key(true)
        .lowercase_keys(true)
        .redact_secrets();
    reg.register(
        Arc::new(deploy.into_indicator(Arc::new(env))),
        RefreshPolicy::new().critical(false),
    );
    reg.start();

    let (_, body) = reg.render();
    assert!(body.contains("\"region\":\"eu-west-1\""), "plain var kept");
    assert!(body.contains("\"api_token\":\"***\""), "secret masked");
    assert!(!body.contains("live-abc123"), "raw secret never rendered");
}

#[cfg(feature = "datetime")]
#[test]
fn datetime_stamp_is_rfc3339() {
    let mut reg = HealthRegistry::with_config(HealthConfig::sync());
    reg.register_fn("clock-probe", RefreshPolicy::new(), ComponentHealth::up);
    reg.start();

    let (_, body) = reg.render();
    let key = "\"refreshed_at\":\"";
    let start = body.find(key).expect("refreshed_at present") + key.len();
    let end = start + body[start..].find('"').expect("closing quote");
    let stamp = &body[start..end];
    assert!(stamp.contains('T') && stamp.ends_with('Z'), "RFC3339 UTC");
    assert_eq!(stamp.len(), 20, "YYYY-MM-DDTHH:MM:SSZ");
}

#[cfg(feature = "otel")]
#[test]
fn otel_bridge_and_listener_see_flips() {
    use crate::{OtelEventBridge, on_status_change};
    use std::sync::atomic::AtomicUsize;

    let mut reg = HealthRegistry::with_config(HealthConfig::sync());
    let transitions = Arc::new(AtomicUsize::new(0));
    {
        let transitions = Arc::clone(&transitions);
        reg.add_listener(on_status_change(move |_| {
            transitions.fetch_add(1, Ordering::Relaxed);
        }));
    }
    reg.add_bridge(Arc::new(OtelEventBridge::new()));

    let venue_up = Arc::new(AtomicBool::new(true));
    {
        let venue_up = Arc::clone(&venue_up);
        reg.register_fn("exchange-session", RefreshPolicy::new(), move || {
            if venue_up.load(Ordering::Relaxed) {
                ComponentHealth::up()
            } else {
                ComponentHealth::down("venue disconnect")
            }
        });
    }
    reg.start();
    assert_eq!(transitions.load(Ordering::Relaxed), 0, "baseline only");

    venue_up.store(false, Ordering::Relaxed);
    reg.refresh_now();
    assert!(
        transitions.load(Ordering::Relaxed) >= 1,
        "flip emits an event"
    );
}
