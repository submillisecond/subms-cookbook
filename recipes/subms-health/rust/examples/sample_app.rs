//! Sample app: a tour of `subms-health`, base API first, then each optional
//! feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or `--features otel` / `--features datetime`) to see the
//! feature sections light up.
//!
//! The scenario is a trading gateway's `/health`. Its readiness rolls up from a
//! market-data feed, a pre-trade risk check, and its exchange session, plus a
//! non-critical order-book cache and a redacted deploy section.
//!
//! * base     - worst-wins rollup, probe-aware codes, non-critical demotion, redaction
//! * datetime - chrono-backed RFC3339 `refreshed_at` on the system clock
//! * otel     - status flips forwarded to OTEL via the subms-otel bridge

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use subms_health::{
    ComponentHealth, EnvSection, HealthConfig, HealthRegistry, HealthStatus, MapEnv, ProbeKind,
    RefreshPolicy,
};

fn main() {
    base_gateway_readiness();

    #[cfg(feature = "datetime")]
    datetime_timestamps();

    #[cfg(feature = "otel")]
    otel_status_export();
}

/// Base API: a trading gateway serves `/health` off a cached snapshot. Its
/// readiness is the worst of its parts, the HTTP code is probe-aware (a degraded
/// process is pulled from rotation but not restarted), a non-critical failure is
/// demoted so the gateway keeps serving, and the deploy section masks secrets.
fn base_gateway_readiness() {
    println!("== base: trading gateway readiness ==");

    let session_up = Arc::new(AtomicBool::new(true));
    let risk_tight = Arc::new(AtomicBool::new(false));

    // Sync config: no background threads, we drive refresh_now() ourselves - the
    // lowest-latency serve path.
    let mut reg = HealthRegistry::with_config(HealthConfig::sync());

    let ready_critical = RefreshPolicy::new().with_kinds(&[ProbeKind::Readiness]);
    reg.register_fn("market-data-feed", ready_critical.clone(), || {
        ComponentHealth::up().with_detail("msgs_per_sec", 184_000i64)
    });

    {
        let risk_tight = Arc::clone(&risk_tight);
        reg.register_fn("risk-check", ready_critical.clone(), move || {
            if risk_tight.load(Ordering::Relaxed) {
                ComponentHealth::degraded("pre-trade limit 92% utilised")
            } else {
                ComponentHealth::up().with_detail("limit_utilisation", "0.41")
            }
        });
    }

    {
        let session_up = Arc::clone(&session_up);
        reg.register_fn(
            "exchange-session",
            RefreshPolicy::new().with_kinds(&[ProbeKind::Liveness, ProbeKind::Readiness]),
            move || {
                if session_up.load(Ordering::Relaxed) {
                    ComponentHealth::up().with_detail("venue", "XLON")
                } else {
                    ComponentHealth::down("gateway logged out")
                }
            },
        );
    }

    // Non-critical: a cold order-book cache never fails readiness, it is demoted
    // to WARN so the gateway keeps serving.
    reg.register_fn(
        "orderbook-cache",
        RefreshPolicy::new().critical(false),
        || ComponentHealth::down("warm cache miss"),
    );

    let env = MapEnv::new()
        .with("KICKSTART_ENV", "prod")
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

    reg.start(); // sync mode: warms the cache, no thread spawned

    // Steady state: the non-critical cache is DOWN but demoted to WARN, so the
    // gateway is still serving (HTTP 200) even though the overall status is WARN.
    let (code, body) = reg.render();
    println!(
        "  steady:   overall {} -> HTTP {code}",
        reg.status().as_str()
    );
    assert_eq!(
        reg.status(),
        HealthStatus::Warn,
        "cache demoted, not failed"
    );
    assert_eq!(code, 200);
    assert!(body.contains("\"region\":\"eu-west-1\""), "plain var kept");
    assert!(body.contains("\"api_token\":\"***\""), "secret masked");
    assert!(!body.contains("live-abc123"), "raw secret never rendered");

    // Pre-trade limits tighten: risk-check goes DEGRADED. Readiness pulls the pod
    // from rotation (503), but liveness stays 200 - a degraded gateway must not be
    // restarted while it is still draining orders.
    risk_tight.store(true, Ordering::Relaxed);
    reg.refresh_now();
    let (ready_code, _) = reg.render_readiness();
    let (live_code, _) = reg.render_liveness();
    println!(
        "  degraded: overall {} -> ready {ready_code}, live {live_code}",
        reg.status().as_str()
    );
    assert_eq!(reg.status(), HealthStatus::Degraded, "worst-wins over WARN");
    assert_eq!(ready_code, 503, "pulled from rotation");
    assert_eq!(live_code, 200, "not restarted");

    // The exchange session drops entirely: DOWN wins worst-wins over DEGRADED, and
    // now even liveness 503s - restart the pod.
    session_up.store(false, Ordering::Relaxed);
    reg.refresh_now();
    let (live_code, _) = reg.render_liveness();
    println!(
        "  down:     overall {} -> live {live_code}",
        reg.status().as_str()
    );
    assert_eq!(reg.status(), HealthStatus::Down, "DOWN beats DEGRADED");
    assert_eq!(live_code, 503, "hard down restarts the pod");

    // Recovery: session back, risk within limits. Overall settles back to WARN
    // (the cache is still cold) and the gateway serves again.
    session_up.store(true, Ordering::Relaxed);
    risk_tight.store(false, Ordering::Relaxed);
    reg.refresh_now();
    let (code, _) = reg.render();
    println!(
        "  recover:  overall {} -> HTTP {code}",
        reg.status().as_str()
    );
    assert_eq!(code, 200, "serving again");
}

/// `datetime` feature: the system clock stamps `refreshed_at` via chrono's
/// RFC3339 formatter (the zero-dep default hand-rolls the same civil-date shape).
/// Either way the stamp is UTC RFC3339, `YYYY-MM-DDTHH:MM:SSZ`.
#[cfg(feature = "datetime")]
fn datetime_timestamps() {
    println!("\n== datetime: chrono RFC3339 timestamps ==");
    let mut reg = HealthRegistry::with_config(HealthConfig::sync());
    reg.register_fn("clock-probe", RefreshPolicy::new(), ComponentHealth::up);
    reg.start();

    let (_, body) = reg.render();
    let stamp = extract_refreshed_at(&body);
    println!("  refreshed_at = {stamp}");
    assert!(stamp.contains('T') && stamp.ends_with('Z'), "RFC3339 UTC");
    assert_eq!(stamp.len(), 20, "YYYY-MM-DDTHH:MM:SSZ");
}

#[cfg(feature = "datetime")]
fn extract_refreshed_at(json: &str) -> String {
    let key = "\"refreshed_at\":\"";
    let start = json.find(key).expect("refreshed_at present") + key.len();
    let end = start + json[start..].find('"').expect("closing quote");
    json[start..end].to_string()
}

/// `otel` feature: status transitions fan out through the registry's dispatcher.
/// A plain listener can observe them in-process, and the subms-otel
/// `OtelEventBridge` forwards each one to OpenTelemetry as a counter (a no-op
/// until a meter provider is installed, so it is always safe to attach).
#[cfg(feature = "otel")]
fn otel_status_export() {
    use std::sync::atomic::AtomicUsize;
    use subms_health::{OtelEventBridge, on_status_change};

    println!("\n== otel: forward status flips to OpenTelemetry ==");
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

    reg.start(); // first rebuild sets the baseline - no spurious events
    assert_eq!(transitions.load(Ordering::Relaxed), 0, "baseline only");

    venue_up.store(false, Ordering::Relaxed);
    reg.refresh_now(); // overall + component both flip UP -> DOWN
    let n = transitions.load(Ordering::Relaxed);
    println!("  observed {n} transitions (also forwarded to OTEL)");
    assert!(n >= 1, "a status flip emits a transition event");
}
