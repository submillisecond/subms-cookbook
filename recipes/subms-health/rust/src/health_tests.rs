use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

use super::*;

// ---- test helpers ----

struct MutableClock {
    ms: AtomicU64,
    stamp: String,
}
impl MutableClock {
    fn new(ms: u64) -> Self {
        Self {
            ms: AtomicU64::new(ms),
            stamp: "2026-06-18T00:00:00Z".to_string(),
        }
    }
    fn set(&self, ms: u64) {
        self.ms.store(ms, Ordering::SeqCst);
    }
}
impl Clock for MutableClock {
    fn now_ms(&self) -> u64 {
        self.ms.load(Ordering::SeqCst)
    }
    fn now_rfc3339(&self) -> String {
        self.stamp.clone()
    }
}

fn fixed_reg() -> HealthRegistry {
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(1000, "2026-06-18T00:00:00Z"));
    HealthRegistry::with_clock(clock)
}

// ---- status ----

#[test]
fn status_aggregate_worst_wins() {
    use HealthStatus::*;
    assert_eq!(HealthStatus::aggregate([]), Up);
    assert_eq!(HealthStatus::aggregate([Up, Up]), Up);
    assert_eq!(HealthStatus::aggregate([Up, Unknown]), Unknown);
    assert_eq!(HealthStatus::aggregate([Unknown, Degraded]), Degraded);
    assert_eq!(HealthStatus::aggregate([Degraded, Down]), Down);
    assert_eq!(HealthStatus::aggregate([Down, Up, Degraded]), Down);
}

#[test]
fn status_tokens_and_http_mapping() {
    assert_eq!(HealthStatus::Up.as_str(), "UP");
    assert_eq!(HealthStatus::Down.as_str(), "DOWN");
    assert_eq!(HealthStatus::Degraded.as_str(), "DEGRADED");
    assert_eq!(HealthStatus::Unknown.as_str(), "UNKNOWN");
    assert_eq!(http_status_for(HealthStatus::Up), 200);
    assert_eq!(http_status_for(HealthStatus::Unknown), 200);
    assert_eq!(http_status_for(HealthStatus::Down), 503);
    assert_eq!(http_status_for(HealthStatus::Degraded), 503);
}

// ---- component ----

#[test]
fn component_builders() {
    let c = ComponentHealth::up()
        .with_detail("ping", "ok")
        .with_detail("n", 3i64);
    assert_eq!(c.status, HealthStatus::Up);
    assert_eq!(c.details.len(), 2);

    let d = ComponentHealth::down("boom");
    assert_eq!(d.status, HealthStatus::Down);
    assert_eq!(
        d.to_json(),
        "{\"status\":\"DOWN\",\"details\":{\"error\":\"boom\"}}"
    );

    let g = ComponentHealth::degraded("slow");
    assert_eq!(g.status, HealthStatus::Degraded);
}

#[test]
fn nested_effective_status() {
    let parent = ComponentHealth::up()
        .with_subcomponent("a", ComponentHealth::up())
        .with_subcomponent("b", ComponentHealth::down("x"));
    assert_eq!(parent.status, HealthStatus::Up);
    assert_eq!(parent.effective_status(), HealthStatus::Down);
}

// ---- registry aggregation + critical demotion ----

#[test]
fn registry_all_up() {
    let mut reg = fixed_reg();
    reg.register_fn("a", RefreshPolicy::new(), ComponentHealth::up);
    reg.register_fn("b", RefreshPolicy::new(), ComponentHealth::up);
    let (code, _) = reg.render();
    assert_eq!(code, 200);
    assert_eq!(reg.status(), HealthStatus::Up);
}

#[test]
fn registry_critical_down_is_down() {
    let mut reg = fixed_reg();
    reg.register_fn("db", RefreshPolicy::new().critical(true), || {
        ComponentHealth::down("gone")
    });
    let (code, _) = reg.render();
    assert_eq!(code, 503);
    assert_eq!(reg.status(), HealthStatus::Down);
}

#[test]
fn registry_non_critical_down_is_warn() {
    let mut reg = fixed_reg();
    reg.register_fn("cache", RefreshPolicy::new().critical(false), || {
        ComponentHealth::down("gone")
    });
    // Non-critical failure -> WARN -> 200 (keep serving), while the component
    // still reports its real DOWN status.
    let (code, json) = reg.render();
    assert_eq!(code, 200);
    assert_eq!(reg.status(), HealthStatus::Warn);
    assert!(json.contains("\"status\":\"DOWN\""));
}

// ---- probe kinds ----

#[test]
fn probe_kind_filtering() {
    let mut reg = fixed_reg();
    reg.register_fn(
        "live-only",
        RefreshPolicy::new().with_kinds(&[ProbeKind::Liveness]),
        ComponentHealth::up,
    );
    reg.register_fn(
        "ready-only",
        RefreshPolicy::new().with_kinds(&[ProbeKind::Readiness]),
        || ComponentHealth::down("not ready"),
    );
    // /health/live sees only the live indicator -> UP
    let (live_code, live_json) = reg.render_liveness();
    assert_eq!(live_code, 200);
    assert!(live_json.contains("live-only"));
    assert!(!live_json.contains("ready-only"));
    // /health/ready sees only the readiness indicator -> DOWN
    let (ready_code, ready_json) = reg.render_readiness();
    assert_eq!(ready_code, 503);
    assert!(ready_json.contains("ready-only"));
    // startup subset is empty -> UP
    let (started_code, _) = reg.render_startup();
    assert_eq!(started_code, 200);
}

#[test]
fn degraded_fails_readiness_not_liveness() {
    let mut reg = fixed_reg();
    reg.register_fn(
        "engine",
        RefreshPolicy::new()
            .critical(true)
            .with_kinds(&[ProbeKind::Liveness, ProbeKind::Readiness]),
        || ComponentHealth::degraded("backpressure"),
    );
    // Readiness pulls it from rotation...
    let (ready_code, _) = reg.render_readiness();
    assert_eq!(ready_code, 503);
    // ...but liveness must NOT 503 (that would restart the pod).
    let (live_code, _) = reg.render_liveness();
    assert_eq!(live_code, 200);
}

// ---- env section ----

#[test]
fn env_explicit_keys() {
    let env = MapEnv::new()
        .with("KICKSTART_ENV", "prod")
        .with("UNRELATED", "x");
    let c = EnvSection::new("deploy")
        .keys(["KICKSTART_ENV"])
        .render(&env);
    assert_eq!(c.details.len(), 1);
    assert!(c.details.contains_key("KICKSTART_ENV"));
}

#[test]
fn env_prefix_and_glob() {
    let env = MapEnv::new()
        .with("KICKSTART_A", "1")
        .with("KICKSTART_B", "2")
        .with("APP_URL", "http://x")
        .with("OTHER", "no");
    let p = EnvSection::new("d").prefix("KICKSTART_").render(&env);
    assert_eq!(p.details.len(), 2);
    let g = EnvSection::new("d").glob("*_URL").render(&env);
    assert_eq!(g.details.len(), 1);
    assert!(g.details.contains_key("APP_URL"));
}

#[test]
fn env_redaction_policies() {
    let v = "supersecretvalue";
    assert_eq!(RedactionPolicy::Mask.apply(v), "***");
    assert_eq!(RedactionPolicy::Last4.apply(v), "***alue");
    assert_eq!(RedactionPolicy::Mask.apply("abc"), "***");
    // hash + fingerprint are deterministic
    assert_eq!(
        RedactionPolicy::Hash.apply(v),
        RedactionPolicy::Hash.apply(v)
    );
    let fp = RedactionPolicy::Fingerprint.apply(v);
    assert!(fp.starts_with("fp_") && fp.len() == 9);
    assert!(RedactionPolicy::Hash.apply(v).starts_with("fnv1a:"));
    // different inputs -> different fingerprints
    assert_ne!(
        RedactionPolicy::Fingerprint.apply("a"),
        RedactionPolicy::Fingerprint.apply("b")
    );
}

#[test]
fn env_remap_strip_lowercase_precedence() {
    let env = MapEnv::new()
        .with("KICKSTART_TARGET", "edge")
        .with("KICKSTART_ENV", "prod");
    let c = EnvSection::new("d")
        .prefix("KICKSTART_")
        .key("KICKSTART_ENV") // also explicit -> still rendered once
        .strip_prefix_in_key(true)
        .lowercase_keys(true)
        .remap("KICKSTART_TARGET", "where")
        .render(&env);
    assert_eq!(c.details.len(), 2);
    assert_eq!(c.details.get("where").unwrap().clone(), "edge".into());
    assert!(c.details.contains_key("env"));
}

#[test]
fn component_unknown_status() {
    let c = ComponentHealth::unknown();
    assert_eq!(c.status, HealthStatus::Unknown);
    assert_eq!(http_status_for(c.status), 200);
    assert_eq!(c.to_json(), "{\"status\":\"UNKNOWN\"}");
}

#[test]
fn health_config_builders_set_fields() {
    let cfg = HealthConfig::new()
        .refresh_mode(RefreshMode::Sync)
        .dispatch_mode(DispatchMode::Sync)
        .tick_ms(250)
        .with_stale_factor(2.0);
    assert_eq!(cfg.mode, RefreshMode::Sync);
    assert_eq!(cfg.dispatch, DispatchMode::Sync);
    assert_eq!(cfg.tick_ms, 250);
    assert_eq!(cfg.stale_factor, 2.0);
}

#[test]
fn system_sections_registry_renders_server_and_deploy() {
    let reg = HealthRegistry::with_system_sections();
    reg.refresh_now();
    let (code, json) = reg.render();
    assert_eq!(code, 200); // both sections are non-critical
    assert!(json.contains("\"server\""), "server indicator present: {json}");
    assert!(json.contains("\"pid\""), "server reports pid");
    assert!(json.contains("\"deploy\""), "deploy env section present");
}

#[test]
fn env_section_substring_redaction_include_empty_and_status() {
    let env = MapEnv::new()
        .with("APP_SECRET_URL", "http://secret-host")
        .with("APP_EMPTY", "");
    let section = EnvSection::new("cfg")
        .prefix("APP_")
        .include_empty(true)
        .status(HealthStatus::Warn)
        .redact_substring("SECRET", RedactionPolicy::Mask);
    assert_eq!(section.name(), "cfg");
    let c = section.render(&env);
    assert_eq!(c.status, HealthStatus::Warn);
    assert!(c.details.contains_key("APP_EMPTY"), "empty var included");
    assert_eq!(
        c.details.get("APP_SECRET_URL").unwrap().clone(),
        "***".into(),
        "substring match redacts the secret value"
    );
}

// ---- cross-language fixtures (byte-exact; Java + Python must match) ----

#[test]
fn cross_language_env_section_fixture() {
    let env = MapEnv::new()
        .with("KICKSTART_ENV", "prod")
        .with("KICKSTART_VERSION", "1.2.3")
        .with("KICKSTART_TOKEN", "supersecret")
        .with("OTHER", "ignore");
    let section = EnvSection::new("deploy")
        .prefix("KICKSTART_")
        .strip_prefix_in_key(true)
        .lowercase_keys(true)
        .redact_secrets();
    let json = section.render(&env).to_json();
    assert_eq!(
        json,
        "{\"status\":\"UP\",\"details\":{\"env\":\"prod\",\"token\":\"***\",\"version\":\"1.2.3\"}}"
    );
}

#[test]
fn cross_language_report_fixture() {
    let mut reg = fixed_reg();
    reg.register_fn("db", RefreshPolicy::new().critical(true), || {
        ComponentHealth::up().with_detail("ping", "ok")
    });
    reg.register_fn("cache", RefreshPolicy::new().critical(false), || {
        ComponentHealth::down("conn refused")
    });
    let (code, json) = reg.render();
    assert_eq!(code, 200);
    assert_eq!(
        json,
        "{\"status\":\"WARN\",\"refreshed_at\":\"2026-06-18T00:00:00Z\",\"components\":{\"cache\":{\"status\":\"DOWN\",\"age_ms\":0,\"stale\":false,\"details\":{\"error\":\"conn refused\"}},\"db\":{\"status\":\"UP\",\"age_ms\":0,\"stale\":false,\"details\":{\"ping\":\"ok\"}}}}"
    );
}

#[test]
fn json_escaping() {
    let c = ComponentHealth::up().with_detail("msg", "a\"b\\c\nd\te");
    let json = c.to_json();
    assert!(json.contains("a\\\"b\\\\c\\nd\\te"));
}

// ---- registry edges ----

#[test]
fn empty_registry() {
    let reg = fixed_reg();
    let (code, json) = reg.render();
    assert_eq!(code, 200);
    assert_eq!(
        json,
        "{\"status\":\"UP\",\"refreshed_at\":\"2026-06-18T00:00:00Z\"}"
    );
}

#[test]
fn staleness_flag() {
    let clock = Arc::new(MutableClock::new(1000));
    let mut reg = HealthRegistry::with_clock(clock.clone()).with_stale_factor(0.5);
    reg.register_fn(
        "x",
        RefreshPolicy::new().with_interval_ms(100),
        ComponentHealth::up,
    );
    reg.refresh_now(); // probed at t=1000, age 0
    clock.set(1080); // age 80, below the 100ms interval -> not re-probed
    reg.refresh_due();
    let (_, json) = reg.render();
    assert!(json.contains("\"age_ms\":80"));
    assert!(json.contains("\"stale\":true")); // 80 > 0.5 * 100
}

#[test]
fn refresh_picks_up_mutation() {
    let down = Arc::new(AtomicBool::new(false));
    let d2 = Arc::clone(&down);
    let mut reg = fixed_reg();
    reg.register_fn("flappy", RefreshPolicy::new().critical(true), move || {
        if d2.load(Ordering::SeqCst) {
            ComponentHealth::down("flipped")
        } else {
            ComponentHealth::up()
        }
    });
    assert_eq!(reg.status(), HealthStatus::Up);
    down.store(true, Ordering::SeqCst);
    reg.refresh_now();
    assert_eq!(reg.status(), HealthStatus::Down);
}

#[test]
fn background_refresher_render_is_non_blocking() {
    // A slow probe runs on the background thread; render() reads the cache and
    // must not serialise behind the 40ms probe.
    let mut reg = HealthRegistry::with_config(HealthConfig::async_every_ms(1));
    reg.register_fn("slow", RefreshPolicy::new().with_interval_ms(0), || {
        std::thread::sleep(std::time::Duration::from_millis(40));
        ComponentHealth::up()
    });
    reg.start(); // initial force refresh blocks ~40ms, then thread keeps going
    let t0 = Instant::now();
    for _ in 0..20 {
        let (code, _) = reg.render();
        assert_eq!(code, 200);
    }
    let elapsed = t0.elapsed();
    assert!(elapsed.as_millis() < 200, "20 renders took {elapsed:?}");
    reg.stop(); // clean join
}

#[test]
fn status_change_callback_fires_non_blocking() {
    use crate::{Event, on_status_change};
    use std::sync::Mutex;

    let events: Arc<Mutex<Vec<Event>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&events);

    let down = Arc::new(AtomicBool::new(false));
    let d2 = Arc::clone(&down);
    let mut reg = fixed_reg();
    reg.register_fn("api", RefreshPolicy::new().critical(true), move || {
        if d2.load(Ordering::SeqCst) {
            ComponentHealth::down("503s")
        } else {
            ComponentHealth::up()
        }
    });
    reg.add_listener(on_status_change(move |e| {
        sink.lock().unwrap().push(e.clone())
    }));

    reg.refresh_now(); // baseline UP, no event
    down.store(true, Ordering::SeqCst);
    reg.refresh_now(); // UP -> DOWN
    down.store(false, Ordering::SeqCst);
    reg.refresh_now(); // DOWN -> UP

    // Dispatcher is async; poll briefly for the two overall transitions to arrive.
    let mut overall = Vec::new();
    for _ in 0..100 {
        overall = events
            .lock()
            .unwrap()
            .iter()
            .filter(|e| e.attr("scope") == Some("overall"))
            .cloned()
            .collect();
        if overall.len() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    reg.stop();
    assert_eq!(overall.len(), 2, "expected two overall transitions");
    assert_eq!(overall[0].topic, "subms.health.status");
    assert_eq!(overall[0].attr("from"), Some("UP"));
    assert_eq!(overall[0].attr("to"), Some("DOWN"));
    assert_eq!(overall[1].attr("from"), Some("DOWN"));
    assert_eq!(overall[1].attr("to"), Some("UP"));
}

#[test]
fn sync_dispatch_has_no_threads() {
    use crate::on_status_change;
    use std::sync::Mutex;

    // Sync config: no refresher thread, no dispatcher thread. Listeners run
    // inline on whoever calls refresh.
    let clock: Arc<dyn Clock> = Arc::new(FixedClock::new(1000, "2026-06-18T00:00:00Z"));
    let mut reg = HealthRegistry::with_config_and_clock(HealthConfig::sync(), clock);
    let hits: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&hits);
    reg.add_listener(on_status_change(move |e| {
        sink.lock().unwrap().push(format!(
            "{}:{}",
            e.attr("scope").unwrap_or(""),
            e.attr("to").unwrap_or("")
        ))
    }));

    let down = Arc::new(AtomicBool::new(false));
    let d2 = Arc::clone(&down);
    reg.register_fn("svc", RefreshPolicy::new().critical(true), move || {
        if d2.load(Ordering::SeqCst) {
            ComponentHealth::down("x")
        } else {
            ComponentHealth::up()
        }
    });

    reg.refresh_now(); // baseline
    down.store(true, Ordering::SeqCst);
    reg.refresh_now(); // change -> listeners fire INLINE (synchronously), no thread

    // No polling needed: sync dispatch means the callback already ran.
    let fired = hits.lock().unwrap().clone();
    assert!(fired.iter().any(|e| e == "overall:DOWN"));
    assert!(fired.iter().any(|e| e == "svc:DOWN"));

    // render_arc serves cached bytes with no String allocation.
    let (code, json) = reg.render_arc();
    assert_eq!(code, 503);
    assert!(json.contains("\"svc\""));
}

#[test]
fn stress_many_indicators() {
    let mut reg = fixed_reg();
    let mut seed: u64 = 0x9e3779b97f4a7c15;
    let mut expected_down = false;
    for i in 0..1000 {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        let down = seed % 50 == 0;
        let critical = i % 3 == 0;
        if down && critical {
            expected_down = true;
        }
        reg.register_fn(
            &format!("i{i}"),
            RefreshPolicy::new().critical(critical),
            move || {
                if down {
                    ComponentHealth::down("x")
                } else {
                    ComponentHealth::up()
                }
            },
        );
    }
    let status = reg.status();
    if expected_down {
        assert_eq!(status, HealthStatus::Down);
    } else {
        assert!(status == HealthStatus::Up || status == HealthStatus::Warn);
    }
    let (_, json) = reg.render();
    assert!(json.contains("\"refreshed_at\""));
}
