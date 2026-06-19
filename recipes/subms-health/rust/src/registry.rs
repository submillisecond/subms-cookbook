//! The registry: holds indicators + per-indicator refresh policy, runs a stdlib
//! background thread that probes each indicator on its own cadence (off the
//! request path), and serves a pre-rendered cached snapshot. `render` is a cache
//! read - it never probes, never touches a dependency, and stays sub-ms.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use subms_events::{DispatchMode, EmitHandle, Event, EventBridge, EventDispatcher, EventListener};

use crate::clock::{Clock, SystemClock};
use crate::component::{ComponentHealth, HealthIndicator};
use crate::env_section::EnvSection;
use crate::events::{HEALTH_STATUS_TOPIC, level_for};
use crate::json::{JsonValue, push_component_map, push_json_str, push_map};
use crate::status::{HealthStatus, http_status_for};

/// Kubernetes-style probe classes. An indicator may serve several.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ProbeKind {
    Liveness,
    Readiness,
    Startup,
}

/// How the registry refreshes its snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefreshMode {
    /// You drive refreshes yourself (call `refresh_due` / `refresh_now` from your
    /// own loop). `start()` just warms the cache once. No background thread.
    Sync,
    /// A background thread refreshes due indicators every `tick_ms`. `start()`
    /// spawns it.
    Async,
}

/// Registry-level config: refresh mode + cadence, event-dispatch mode, and the
/// staleness factor. The mirror of this struct exists in every language port.
///
/// The lowest-latency, zero-extra-thread setup is
/// `HealthConfig { mode: Sync, dispatch: Sync, .. }`: no background threads at
/// all, you drive `refresh_due()` from your own loop, and `render_arc()` serves
/// the cached bytes with no per-request allocation.
#[derive(Debug, Clone, Copy)]
pub struct HealthConfig {
    pub mode: RefreshMode,
    /// Async refresher cadence (ms). Ignored in Sync mode.
    pub tick_ms: u64,
    pub dispatch: DispatchMode,
    /// Flag a component `stale` once its age exceeds `stale_factor * interval`.
    pub stale_factor: f64,
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            mode: RefreshMode::Async,
            tick_ms: 1_000,
            dispatch: DispatchMode::Async,
            stale_factor: 1.5,
        }
    }
}

impl HealthConfig {
    pub fn new() -> Self {
        Self::default()
    }
    /// No background threads: sync refresh + sync dispatch. The low-latency path.
    pub fn sync() -> Self {
        Self {
            mode: RefreshMode::Sync,
            dispatch: DispatchMode::Sync,
            ..Self::default()
        }
    }
    pub fn async_every_ms(tick_ms: u64) -> Self {
        Self {
            mode: RefreshMode::Async,
            tick_ms,
            ..Self::default()
        }
    }
    pub fn refresh_mode(mut self, mode: RefreshMode) -> Self {
        self.mode = mode;
        self
    }
    pub fn dispatch_mode(mut self, dispatch: DispatchMode) -> Self {
        self.dispatch = dispatch;
        self
    }
    pub fn tick_ms(mut self, tick_ms: u64) -> Self {
        self.tick_ms = tick_ms;
        self
    }
    pub fn with_stale_factor(mut self, factor: f64) -> Self {
        self.stale_factor = factor;
        self
    }
}

/// Per-indicator policy: how often to refresh, which probes it serves, and
/// whether a failure is critical (a non-critical failure is demoted to Warn so
/// the overall report stays serving instead of failing).
#[derive(Debug, Clone)]
pub struct RefreshPolicy {
    pub interval_ms: u64,
    pub probe_kinds: Vec<ProbeKind>,
    pub critical: bool,
}

impl Default for RefreshPolicy {
    fn default() -> Self {
        Self {
            interval_ms: 30_000,
            probe_kinds: vec![ProbeKind::Readiness],
            critical: true,
        }
    }
}

impl RefreshPolicy {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with_interval_ms(mut self, ms: u64) -> Self {
        self.interval_ms = ms;
        self
    }
    pub fn with_kinds(mut self, kinds: &[ProbeKind]) -> Self {
        self.probe_kinds = kinds.to_vec();
        self
    }
    pub fn critical(mut self, critical: bool) -> Self {
        self.critical = critical;
        self
    }
    fn includes(&self, kind: ProbeKind) -> bool {
        self.probe_kinds.contains(&kind)
    }
}

struct Registered {
    indicator: Arc<dyn HealthIndicator>,
    policy: RefreshPolicy,
}

struct Cached {
    component: ComponentHealth,
    refreshed_at_ms: u64,
}

/// One rendered view (a probe subset): status, http code, and the JSON bytes.
/// `json` is an `Arc<str>` so `render_arc` can hand it out with no copy.
#[derive(Clone)]
struct View {
    status: HealthStatus,
    code: u16,
    json: Arc<str>,
}

struct Snapshot {
    all: View,
    live: View,
    ready: View,
    started: View,
}

struct Inner {
    indicators: Mutex<Vec<Registered>>,
    cache: Mutex<Vec<Option<Cached>>>,
    snapshot: Mutex<Option<Arc<Snapshot>>>,
    clock: Arc<dyn Clock>,
    stale_factor: f64,
    emitter: EmitHandle,
    prev_overall: Mutex<Option<HealthStatus>>,
    prev_components: Mutex<BTreeMap<String, HealthStatus>>,
}

struct Refresher {
    stop: Arc<AtomicBool>,
    wake: Arc<Condvar>,
    lock: Arc<Mutex<()>>,
    handle: Option<JoinHandle<()>>,
}

/// A health registry. Build it, register indicators, optionally `start()` the
/// background refresher, then serve `render*()` from your HTTP handler.
pub struct HealthRegistry {
    inner: Arc<Inner>,
    refresher: Option<Refresher>,
    dispatcher: EventDispatcher,
    mode: RefreshMode,
    tick_ms: u64,
}

impl HealthRegistry {
    /// Empty registry with default config on the system clock.
    pub fn new() -> Self {
        Self::with_config(HealthConfig::default())
    }

    /// Build from an explicit config on the system clock.
    pub fn with_config(config: HealthConfig) -> Self {
        Self::with_config_and_clock(config, Arc::new(SystemClock))
    }

    /// Default config with a custom clock (used by tests / fixtures).
    pub fn with_clock(clock: Arc<dyn Clock>) -> Self {
        Self::with_config_and_clock(HealthConfig::default(), clock)
    }

    /// Build from an explicit config + clock.
    pub fn with_config_and_clock(config: HealthConfig, clock: Arc<dyn Clock>) -> Self {
        let dispatcher = EventDispatcher::new(config.dispatch);
        let emitter = dispatcher.handle();
        Self {
            inner: Arc::new(Inner {
                indicators: Mutex::new(Vec::new()),
                cache: Mutex::new(Vec::new()),
                snapshot: Mutex::new(None),
                clock,
                stale_factor: config.stale_factor,
                emitter,
                prev_overall: Mutex::new(None),
                prev_components: Mutex::new(BTreeMap::new()),
            }),
            refresher: None,
            dispatcher,
            mode: config.mode,
            tick_ms: config.tick_ms,
        }
    }

    /// Flag a component `stale` once its age exceeds `factor * interval`. Below 1
    /// is an early warning; above 1 an overdue alarm. Default 1.5. Must be set
    /// before any indicator is registered (it lives in the shared inner state).
    pub fn with_stale_factor(mut self, factor: f64) -> Self {
        if let Some(inner) = Arc::get_mut(&mut self.inner) {
            inner.stale_factor = factor;
        }
        self
    }

    /// A registry pre-loaded with a `server` indicator (pid/host/uptime) and a
    /// `deploy` env section (every `KICKSTART_*` var, secrets masked).
    pub fn with_system_sections() -> Self {
        let mut r = Self::new();
        r.register(
            Arc::new(ServerIndicator::new()),
            RefreshPolicy::new()
                .with_interval_ms(5_000)
                .with_kinds(&[
                    ProbeKind::Liveness,
                    ProbeKind::Readiness,
                    ProbeKind::Startup,
                ])
                .critical(false),
        );
        let deploy = EnvSection::new("deploy")
            .prefix("KICKSTART_")
            .strip_prefix_in_key(true)
            .lowercase_keys(true)
            .redact_secrets();
        r.register_section(
            deploy,
            RefreshPolicy::new()
                .with_interval_ms(60_000)
                .with_kinds(&[
                    ProbeKind::Liveness,
                    ProbeKind::Readiness,
                    ProbeKind::Startup,
                ])
                .critical(false),
        );
        r
    }

    /// Register an indicator with a refresh policy.
    pub fn register(
        &mut self,
        indicator: Arc<dyn HealthIndicator>,
        policy: RefreshPolicy,
    ) -> &mut Self {
        self.inner
            .indicators
            .lock()
            .unwrap()
            .push(Registered { indicator, policy });
        self
    }

    /// Register a closure indicator.
    pub fn register_fn<F>(&mut self, name: &str, policy: RefreshPolicy, f: F) -> &mut Self
    where
        F: Fn() -> ComponentHealth + Send + Sync + 'static,
    {
        self.register(Arc::new(crate::component::indicator(name, f)), policy)
    }

    /// Register an env section bound to the real process environment.
    pub fn register_section(&mut self, section: EnvSection, policy: RefreshPolicy) -> &mut Self {
        self.register(Arc::new(section.into_system_indicator()), policy)
    }

    /// Subscribe to status-change events (overall + per-component transitions,
    /// emitted as `subms-events` events on topic `subms.health.status`). Delivery
    /// honors the configured dispatch mode (async off-thread by default), so a
    /// slow listener never stalls a probe.
    pub fn add_listener(&mut self, listener: Arc<dyn EventListener>) -> &mut Self {
        self.dispatcher.add_listener(listener);
        self
    }

    /// Attach an event bridge (e.g. `subms-otel`'s `OtelEventBridge`) that
    /// forwards status-change events to an external system.
    pub fn add_bridge(&mut self, bridge: Arc<dyn EventBridge>) -> &mut Self {
        self.dispatcher.add_bridge(bridge);
        self
    }

    /// Force-probe every indicator now and rebuild the cached snapshot.
    pub fn refresh_now(&self) {
        rebuild(&self.inner, true);
    }

    /// Probe only the indicators whose interval has elapsed, then rebuild the
    /// snapshot. Call this from your own event loop instead of `start()` if you
    /// already own a scheduler.
    pub fn refresh_due(&self) {
        rebuild(&self.inner, false);
    }

    /// Warm the cache, and in Async mode spawn the background refresher (a
    /// daemon-style thread that wakes every `tick_ms`, probes due indicators, and
    /// swaps in a fresh snapshot). In Sync mode this only warms the cache - you
    /// drive `refresh_due()` yourself. Honors the configured `RefreshMode`.
    pub fn start(&mut self) {
        rebuild(&self.inner, true);
        if self.mode == RefreshMode::Sync || self.refresher.is_some() {
            return;
        }
        let tick_ms = self.tick_ms;
        let stop = Arc::new(AtomicBool::new(false));
        let wake = Arc::new(Condvar::new());
        let lock = Arc::new(Mutex::new(()));
        let inner = Arc::clone(&self.inner);
        let t_stop = Arc::clone(&stop);
        let t_wake = Arc::clone(&wake);
        let t_lock = Arc::clone(&lock);
        let handle = thread::Builder::new()
            .name("subms-health-refresh".to_string())
            .spawn(move || {
                let tick = Duration::from_millis(tick_ms.max(1));
                while !t_stop.load(Ordering::Acquire) {
                    let guard = t_lock.lock().unwrap();
                    let _ = t_wake.wait_timeout(guard, tick);
                    if t_stop.load(Ordering::Acquire) {
                        break;
                    }
                    rebuild(&inner, false);
                }
            })
            .expect("spawn refresher");
        self.refresher = Some(Refresher {
            stop,
            wake,
            lock,
            handle: Some(handle),
        });
    }

    /// Stop the background refresher and the event dispatcher, joining both.
    pub fn stop(&mut self) {
        if let Some(mut r) = self.refresher.take() {
            r.stop.store(true, Ordering::Release);
            {
                let _g = r.lock.lock().unwrap();
                r.wake.notify_all();
            }
            if let Some(h) = r.handle.take() {
                let _ = h.join();
            }
        }
        self.dispatcher.stop();
    }

    fn view_arc(&self, pick: fn(&Snapshot) -> &View) -> (u16, Arc<str>) {
        let need_refresh = self.inner.snapshot.lock().unwrap().is_none();
        if need_refresh {
            rebuild(&self.inner, true);
        }
        let snap = self.inner.snapshot.lock().unwrap();
        let v = pick(snap.as_ref().expect("snapshot present"));
        (v.code, Arc::clone(&v.json))
    }

    /// The full `/health` document: `(http_code, json)`. Cache read, never probes.
    pub fn render(&self) -> (u16, String) {
        let (code, json) = self.view_arc(|s| &s.all);
        (code, json.to_string())
    }
    /// Zero-copy variant of [`render`]: clones an `Arc<str>` instead of allocating
    /// a fresh `String`. The lowest-latency serve path.
    pub fn render_arc(&self) -> (u16, Arc<str>) {
        self.view_arc(|s| &s.all)
    }
    /// `/health/live` - liveness probes only.
    pub fn render_liveness(&self) -> (u16, String) {
        let (code, json) = self.view_arc(|s| &s.live);
        (code, json.to_string())
    }
    /// `/health/ready` - readiness probes only.
    pub fn render_readiness(&self) -> (u16, String) {
        let (code, json) = self.view_arc(|s| &s.ready);
        (code, json.to_string())
    }
    /// `/health/started` - startup probes only.
    pub fn render_startup(&self) -> (u16, String) {
        let (code, json) = self.view_arc(|s| &s.started);
        (code, json.to_string())
    }

    /// Overall status of the full document.
    pub fn status(&self) -> HealthStatus {
        let need_refresh = self.inner.snapshot.lock().unwrap().is_none();
        if need_refresh {
            rebuild(&self.inner, true);
        }
        self.inner
            .snapshot
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .all
            .status
    }
}

impl Default for HealthRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for HealthRegistry {
    fn drop(&mut self) {
        self.stop();
    }
}

fn rebuild(inner: &Inner, force: bool) {
    let now = inner.clock.now_ms();
    let stamp = inner.clock.now_rfc3339();
    let inds = inner.indicators.lock().unwrap();
    let mut cache = inner.cache.lock().unwrap();
    if cache.len() != inds.len() {
        cache.resize_with(inds.len(), || None);
    }
    for i in 0..inds.len() {
        let due = force
            || match &cache[i] {
                None => true,
                Some(c) => now.saturating_sub(c.refreshed_at_ms) >= inds[i].policy.interval_ms,
            };
        if due {
            let component = inds[i].indicator.check();
            cache[i] = Some(Cached {
                component,
                refreshed_at_ms: now,
            });
        }
    }
    let all = build_view(inner, &inds, &cache, now, &stamp, None);
    let live = build_view(inner, &inds, &cache, now, &stamp, Some(ProbeKind::Liveness));
    let ready = build_view(
        inner,
        &inds,
        &cache,
        now,
        &stamp,
        Some(ProbeKind::Readiness),
    );
    let started = build_view(inner, &inds, &cache, now, &stamp, Some(ProbeKind::Startup));

    let overall = all.status;
    let mut current: BTreeMap<String, HealthStatus> = BTreeMap::new();
    for (i, reg) in inds.iter().enumerate() {
        let st = match &cache[i] {
            Some(c) => c.component.effective_status(),
            None => HealthStatus::Unknown,
        };
        current.insert(reg.indicator.name().to_string(), st);
    }
    drop(cache);
    drop(inds);
    *inner.snapshot.lock().unwrap() = Some(Arc::new(Snapshot {
        all,
        live,
        ready,
        started,
    }));

    emit_changes(inner, overall, current, &stamp);
}

/// Diff the new overall + per-component statuses against the previous rebuild and
/// emit a `subms-events` event for each transition. The first rebuild only sets
/// the baseline (no spurious events). The dispatch mode (sync/async) is the
/// dispatcher's; in async mode emit is a non-blocking enqueue.
fn emit_changes(
    inner: &Inner,
    overall: HealthStatus,
    current: BTreeMap<String, HealthStatus>,
    at: &str,
) {
    let mut changes: Vec<(String, HealthStatus, HealthStatus)> = Vec::new();
    {
        let mut prev = inner.prev_overall.lock().unwrap();
        if let Some(old) = *prev {
            if old != overall {
                changes.push(("overall".to_string(), old, overall));
            }
        }
        *prev = Some(overall);
    }
    {
        let mut prev = inner.prev_components.lock().unwrap();
        for (name, st) in &current {
            if let Some(old) = prev.get(name) {
                if old != st {
                    changes.push((name.clone(), *old, *st));
                }
            }
        }
        *prev = current;
    }
    for (scope, from, to) in changes {
        let event = Event::builder(HEALTH_STATUS_TOPIC)
            .level(level_for(to))
            .at(at)
            .message(&format!("{scope}: {} -> {}", from.as_str(), to.as_str()))
            .attr("scope", &scope)
            .attr("from", from.as_str())
            .attr("to", to.as_str())
            .build();
        inner.emitter.emit(event);
    }
}

struct Entry {
    status: HealthStatus,
    age_ms: u64,
    stale: bool,
    component: ComponentHealth,
}

fn build_view(
    inner: &Inner,
    inds: &[Registered],
    cache: &[Option<Cached>],
    now_ms: u64,
    refreshed_at: &str,
    filter: Option<ProbeKind>,
) -> View {
    let mut entries: BTreeMap<String, Entry> = BTreeMap::new();
    let mut contributed: Vec<HealthStatus> = Vec::new();
    for (i, reg) in inds.iter().enumerate() {
        if let Some(k) = filter {
            if !reg.policy.includes(k) {
                continue;
            }
        }
        let (component, refreshed_at_ms) = match &cache[i] {
            Some(c) => (c.component.clone(), c.refreshed_at_ms),
            None => (pending(), now_ms),
        };
        let eff = component.effective_status();
        // A non-critical failure never fails the report: it is demoted to Warn
        // (HTTP 200), so the pod keeps serving. Critical statuses pass through.
        let contrib =
            if !reg.policy.critical && matches!(eff, HealthStatus::Down | HealthStatus::Degraded) {
                HealthStatus::Warn
            } else {
                eff
            };
        contributed.push(contrib);
        let age_ms = now_ms.saturating_sub(refreshed_at_ms);
        let stale = (age_ms as f64) > (reg.policy.interval_ms as f64 * inner.stale_factor);
        entries.insert(
            reg.indicator.name().to_string(),
            Entry {
                status: eff,
                age_ms,
                stale,
                component,
            },
        );
    }
    let status = HealthStatus::aggregate(contributed);
    let json: Arc<str> = Arc::from(serialize_report(status, refreshed_at, &entries));
    View {
        status,
        code: code_for(status, filter),
        json,
    }
}

/// Probe-aware HTTP code. Liveness restarts the pod, so it only 503s on a hard
/// DOWN - a DEGRADED process must not be killed. Readiness 503s on DEGRADED/DOWN
/// (pull from rotation). Startup stays 503 until everything is UP/WARN. The full
/// `/health` document uses the readiness-style mapping ([`http_status_for`]).
fn code_for(status: HealthStatus, filter: Option<ProbeKind>) -> u16 {
    match filter {
        None => http_status_for(status),
        Some(ProbeKind::Liveness) => {
            if status == HealthStatus::Down {
                503
            } else {
                200
            }
        }
        Some(ProbeKind::Readiness) => {
            if matches!(status, HealthStatus::Degraded | HealthStatus::Down) {
                503
            } else {
                200
            }
        }
        Some(ProbeKind::Startup) => {
            if matches!(status, HealthStatus::Up | HealthStatus::Warn) {
                200
            } else {
                503
            }
        }
    }
}

fn pending() -> ComponentHealth {
    let mut c = ComponentHealth::unknown();
    c.details
        .insert("state".to_string(), JsonValue::Str("pending".to_string()));
    c
}

fn serialize_report(
    status: HealthStatus,
    refreshed_at: &str,
    entries: &BTreeMap<String, Entry>,
) -> String {
    let mut out = String::new();
    out.push('{');
    out.push_str("\"status\":");
    push_json_str(&mut out, status.as_str());
    out.push_str(",\"refreshed_at\":");
    push_json_str(&mut out, refreshed_at);
    if !entries.is_empty() {
        out.push_str(",\"components\":{");
        for (i, (name, e)) in entries.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            push_json_str(&mut out, name);
            out.push(':');
            out.push('{');
            out.push_str("\"status\":");
            push_json_str(&mut out, e.status.as_str());
            out.push_str(&format!(",\"age_ms\":{}", e.age_ms));
            out.push_str(",\"stale\":");
            out.push_str(if e.stale { "true" } else { "false" });
            if !e.component.details.is_empty() {
                out.push_str(",\"details\":");
                push_map(&mut out, &e.component.details);
            }
            if !e.component.components.is_empty() {
                out.push_str(",\"components\":");
                push_component_map(&mut out, &e.component.components);
            }
            out.push('}');
        }
        out.push('}');
    }
    out.push('}');
    out
}

/// Built-in `server` indicator: pid, hostname, and process uptime.
pub struct ServerIndicator {
    started: Instant,
    pid: u32,
    hostname: String,
}

impl ServerIndicator {
    pub fn new() -> Self {
        let hostname = std::env::var("HOSTNAME")
            .or_else(|_| std::env::var("COMPUTERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());
        Self {
            started: Instant::now(),
            pid: std::process::id(),
            hostname,
        }
    }
}

impl Default for ServerIndicator {
    fn default() -> Self {
        Self::new()
    }
}

impl HealthIndicator for ServerIndicator {
    fn name(&self) -> &str {
        "server"
    }
    fn check(&self) -> ComponentHealth {
        ComponentHealth::up()
            .with_detail("pid", self.pid as i64)
            .with_detail("hostname", self.hostname.as_str())
            .with_detail("uptime_ms", self.started.elapsed().as_millis() as i64)
    }
}
