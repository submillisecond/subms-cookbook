package com.submillisecond.recipes.health;

import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;

import com.submillisecond.recipes.events.EmitHandle;
import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventBridge;
import com.submillisecond.recipes.events.EventDispatcher;
import com.submillisecond.recipes.events.EventLevel;
import com.submillisecond.recipes.events.EventListener;

/**
 * Holds indicators + per-indicator policy, optionally runs a background
 * refresher that probes off the request path, and serves a pre-rendered cached
 * snapshot. Status changes are emitted as subms-events events.
 */
public final class HealthRegistry {
    public static final String HEALTH_STATUS_TOPIC = "subms.health.status";

    public record Result(int code, String json) {}

    private record Registered(HealthIndicator indicator, RefreshPolicy policy) {}

    private record Cached(ComponentHealth component, long refreshedAtMs) {}

    private record View(HealthStatus status, int code, String json) {}

    private record Snapshot(View all, View live, View ready, View started) {}

    private final HealthConfig config;
    private final Clock clock;
    private final Object lock = new Object();
    private final List<Registered> indicators = new ArrayList<>();
    private final Map<Integer, Cached> cache = new LinkedHashMap<>();
    private volatile Snapshot snapshot;

    private final EventDispatcher dispatcher;
    private final EmitHandle emitter;
    private HealthStatus prevOverall;
    private final Map<String, HealthStatus> prevComponents = new LinkedHashMap<>();

    private ScheduledExecutorService scheduler;

    public HealthRegistry() {
        this(HealthConfig.defaults(), new Clock.SystemClock());
    }

    public HealthRegistry(HealthConfig config) {
        this(config, new Clock.SystemClock());
    }

    public HealthRegistry(HealthConfig config, Clock clock) {
        this.config = config == null ? HealthConfig.defaults() : config;
        this.clock = clock == null ? new Clock.SystemClock() : clock;
        this.dispatcher = new EventDispatcher(this.config.dispatch());
        this.emitter = this.dispatcher.handle();
    }

    public static HealthRegistry withSystemSections() {
        HealthRegistry r = new HealthRegistry();
        r.register(new ServerIndicator(),
                new RefreshPolicy().intervalMs(5_000).kinds(ProbeKind.values()).critical(false));
        EnvSection deploy = new EnvSection("deploy")
                .prefix("KICKSTART_").stripPrefixInKey(true).lowercaseKeys(true).redactSecrets();
        r.register(deploy.intoIndicator(new EnvProvider.SystemEnv()),
                new RefreshPolicy().intervalMs(60_000).kinds(ProbeKind.values()).critical(false));
        return r;
    }

    public HealthRegistry register(HealthIndicator indicator, RefreshPolicy policy) {
        synchronized (lock) {
            indicators.add(new Registered(indicator, policy == null ? new RefreshPolicy() : policy));
        }
        return this;
    }

    public HealthRegistry registerFn(String name, java.util.function.Supplier<ComponentHealth> fn, RefreshPolicy policy) {
        return register(HealthIndicator.of(name, fn), policy);
    }

    public HealthRegistry registerSection(EnvSection section, RefreshPolicy policy) {
        return register(section.intoIndicator(new EnvProvider.SystemEnv()), policy);
    }

    public HealthRegistry addListener(EventListener listener) {
        dispatcher.addListener(listener);
        return this;
    }

    public HealthRegistry addBridge(EventBridge bridge) {
        dispatcher.addBridge(bridge);
        return this;
    }

    public void refreshNow() {
        rebuild(true);
    }

    public void refreshDue() {
        rebuild(false);
    }

    public void start() {
        rebuild(true);
        if (config.mode() == HealthConfig.RefreshMode.SYNC || scheduler != null) {
            return;
        }
        scheduler = Executors.newSingleThreadScheduledExecutor(r -> {
            Thread t = new Thread(r, "subms-health-refresh");
            t.setDaemon(true);
            return t;
        });
        long tick = Math.max(1, config.tickMs());
        scheduler.scheduleAtFixedRate(() -> rebuild(false), tick, tick, TimeUnit.MILLISECONDS);
    }

    public void stop() {
        if (scheduler != null) {
            scheduler.shutdownNow();
            scheduler = null;
        }
        dispatcher.stop();
    }

    private void ensureSnapshot() {
        if (snapshot == null) {
            rebuild(true);
        }
    }

    public Result render() {
        ensureSnapshot();
        View v = snapshot.all();
        return new Result(v.code(), v.json());
    }

    public Result renderLiveness() {
        ensureSnapshot();
        View v = snapshot.live();
        return new Result(v.code(), v.json());
    }

    public Result renderReadiness() {
        ensureSnapshot();
        View v = snapshot.ready();
        return new Result(v.code(), v.json());
    }

    public Result renderStartup() {
        ensureSnapshot();
        View v = snapshot.started();
        return new Result(v.code(), v.json());
    }

    public HealthStatus status() {
        ensureSnapshot();
        return snapshot.all().status();
    }

    private void rebuild(boolean force) {
        HealthStatus overall;
        Map<String, HealthStatus> current = new LinkedHashMap<>();
        synchronized (lock) {
            long now = clock.nowMs();
            String stamp = clock.nowRfc3339();
            List<Registered> regs = new ArrayList<>(indicators);
            for (int i = 0; i < regs.size(); i++) {
                Cached c = cache.get(i);
                boolean due = force || c == null || (now - c.refreshedAtMs()) >= regs.get(i).policy().intervalMs();
                if (due) {
                    cache.put(i, new Cached(regs.get(i).indicator().check(), now));
                }
            }
            View all = buildView(regs, now, stamp, null);
            View live = buildView(regs, now, stamp, ProbeKind.LIVENESS);
            View ready = buildView(regs, now, stamp, ProbeKind.READINESS);
            View started = buildView(regs, now, stamp, ProbeKind.STARTUP);
            snapshot = new Snapshot(all, live, ready, started);
            overall = all.status();
            for (int i = 0; i < regs.size(); i++) {
                Cached c = cache.get(i);
                current.put(regs.get(i).indicator().name(),
                        c != null ? c.component().effectiveStatus() : HealthStatus.UNKNOWN);
            }
        }
        emitChanges(overall, current, clock.nowRfc3339());
    }

    private View buildView(List<Registered> regs, long now, String stamp, ProbeKind filter) {
        TreeMap<String, Object[]> entries = new TreeMap<>();
        List<HealthStatus> contributed = new ArrayList<>();
        for (int i = 0; i < regs.size(); i++) {
            Registered reg = regs.get(i);
            if (filter != null && !reg.policy().includes(filter)) {
                continue;
            }
            Cached c = cache.get(i);
            ComponentHealth comp;
            long refreshedAt;
            if (c != null) {
                comp = c.component();
                refreshedAt = c.refreshedAtMs();
            } else {
                comp = ComponentHealth.unknown().withDetail("state", "pending");
                refreshedAt = now;
            }
            HealthStatus eff = comp.effectiveStatus();
            HealthStatus contrib = (!reg.policy().isCritical()
                    && (eff == HealthStatus.DOWN || eff == HealthStatus.DEGRADED))
                    ? HealthStatus.WARN
                    : eff;
            contributed.add(contrib);
            long ageMs = Math.max(0, now - refreshedAt);
            boolean stale = ageMs > reg.policy().intervalMs() * config.staleFactor();
            entries.put(reg.indicator().name(), new Object[] {eff, ageMs, stale, comp});
        }
        HealthStatus status = HealthStatus.aggregate(contributed);
        String json = serializeReport(status, stamp, entries);
        return new View(status, codeFor(status, filter), json);
    }

    private static int codeFor(HealthStatus status, ProbeKind filter) {
        if (filter == null) {
            return HealthStatus.httpStatusFor(status);
        }
        return switch (filter) {
            case LIVENESS -> status == HealthStatus.DOWN ? 503 : 200;
            case READINESS -> (status == HealthStatus.DEGRADED || status == HealthStatus.DOWN) ? 503 : 200;
            case STARTUP -> (status == HealthStatus.UP || status == HealthStatus.WARN) ? 200 : 503;
        };
    }

    private static String serializeReport(HealthStatus status, String refreshedAt, TreeMap<String, Object[]> entries) {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"status\":");
        Json.escape(sb, status.token());
        sb.append(",\"refreshed_at\":");
        Json.escape(sb, refreshedAt);
        if (!entries.isEmpty()) {
            sb.append(",\"components\":{");
            boolean first = true;
            for (Map.Entry<String, Object[]> e : entries.entrySet()) {
                if (!first) {
                    sb.append(',');
                }
                first = false;
                Json.escape(sb, e.getKey());
                sb.append(":{\"status\":");
                Object[] v = e.getValue();
                Json.escape(sb, ((HealthStatus) v[0]).token());
                sb.append(",\"age_ms\":").append((long) v[1]);
                sb.append(",\"stale\":").append(((boolean) v[2]) ? "true" : "false");
                ComponentHealth comp = (ComponentHealth) v[3];
                if (!comp.details().isEmpty()) {
                    sb.append(",\"details\":");
                    Json.map(sb, comp.details());
                }
                if (!comp.components().isEmpty()) {
                    sb.append(",\"components\":{");
                    boolean f2 = true;
                    for (Map.Entry<String, ComponentHealth> ce : comp.components().entrySet()) {
                        if (!f2) {
                            sb.append(',');
                        }
                        f2 = false;
                        Json.escape(sb, ce.getKey());
                        sb.append(':');
                        ce.getValue().writeJson(sb);
                    }
                    sb.append('}');
                }
                sb.append('}');
            }
            sb.append('}');
        }
        sb.append('}');
        return sb.toString();
    }

    private void emitChanges(HealthStatus overall, Map<String, HealthStatus> current, String at) {
        List<Object[]> changes = new ArrayList<>();
        if (prevOverall != null && prevOverall != overall) {
            changes.add(new Object[] {"overall", prevOverall, overall});
        }
        prevOverall = overall;
        for (Map.Entry<String, HealthStatus> e : current.entrySet()) {
            HealthStatus old = prevComponents.get(e.getKey());
            if (old != null && old != e.getValue()) {
                changes.add(new Object[] {e.getKey(), old, e.getValue()});
            }
        }
        prevComponents.clear();
        prevComponents.putAll(current);
        for (Object[] ch : changes) {
            String scope = (String) ch[0];
            HealthStatus from = (HealthStatus) ch[1];
            HealthStatus to = (HealthStatus) ch[2];
            Event event = Event.builder(HEALTH_STATUS_TOPIC)
                    .level(levelFor(to))
                    .at(at)
                    .message(scope + ": " + from.token() + " -> " + to.token())
                    .attr("scope", scope)
                    .attr("from", from.token())
                    .attr("to", to.token())
                    .build();
            emitter.emit(event);
        }
    }

    private static EventLevel levelFor(HealthStatus status) {
        return switch (status) {
            case DOWN -> EventLevel.ERROR;
            case DEGRADED, WARN -> EventLevel.WARN;
            case UP, UNKNOWN -> EventLevel.INFO;
        };
    }
}
