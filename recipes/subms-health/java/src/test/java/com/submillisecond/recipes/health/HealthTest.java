package com.submillisecond.recipes.health;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

import org.junit.jupiter.api.Test;

class HealthTest {

    private static HealthRegistry fixedReg() {
        return new HealthRegistry(HealthConfig.defaults(), new Clock.FixedClock(1000, "2026-06-18T00:00:00Z"));
    }

    private static HealthRegistry fixedReg(HealthConfig config) {
        return new HealthRegistry(config, new Clock.FixedClock(1000, "2026-06-18T00:00:00Z"));
    }

    @Test
    void aggregateWorstWins() {
        assertEquals(HealthStatus.UP, HealthStatus.aggregate(List.of()));
        assertEquals(HealthStatus.UNKNOWN, HealthStatus.aggregate(List.of(HealthStatus.UP, HealthStatus.UNKNOWN)));
        assertEquals(HealthStatus.WARN, HealthStatus.aggregate(List.of(HealthStatus.UNKNOWN, HealthStatus.WARN)));
        assertEquals(HealthStatus.DEGRADED, HealthStatus.aggregate(List.of(HealthStatus.WARN, HealthStatus.DEGRADED)));
        assertEquals(HealthStatus.DOWN, HealthStatus.aggregate(List.of(HealthStatus.DEGRADED, HealthStatus.DOWN)));
    }

    @Test
    void tokensAndHttpMapping() {
        assertEquals("WARN", HealthStatus.WARN.token());
        assertEquals(200, HealthStatus.httpStatusFor(HealthStatus.UP));
        assertEquals(200, HealthStatus.httpStatusFor(HealthStatus.WARN));
        assertEquals(503, HealthStatus.httpStatusFor(HealthStatus.DEGRADED));
        assertEquals(503, HealthStatus.httpStatusFor(HealthStatus.DOWN));
    }

    @Test
    void componentBuildersJson() {
        ComponentHealth d = ComponentHealth.down("boom");
        assertEquals(HealthStatus.DOWN, d.status());
        assertEquals("{\"status\":\"DOWN\",\"details\":{\"error\":\"boom\"}}", d.toJson());
    }

    @Test
    void nestedEffectiveStatus() {
        ComponentHealth p = ComponentHealth.up().withSubcomponent("b", ComponentHealth.down("x"));
        assertEquals(HealthStatus.UP, p.status());
        assertEquals(HealthStatus.DOWN, p.effectiveStatus());
    }

    @Test
    void registryAllUp() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("a", ComponentHealth::up, new RefreshPolicy());
        assertEquals(200, reg.render().code());
        assertEquals(HealthStatus.UP, reg.status());
    }

    @Test
    void criticalDownIsDown() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("db", () -> ComponentHealth.down("gone"), new RefreshPolicy().critical(true));
        assertEquals(503, reg.render().code());
        assertEquals(HealthStatus.DOWN, reg.status());
    }

    @Test
    void nonCriticalDownIsWarn() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("cache", () -> ComponentHealth.down("gone"), new RefreshPolicy().critical(false));
        HealthRegistry.Result r = reg.render();
        assertEquals(200, r.code());
        assertEquals(HealthStatus.WARN, reg.status());
        assertTrue(r.json().contains("\"status\":\"DOWN\""));
    }

    @Test
    void probeKindFiltering() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("live", ComponentHealth::up, new RefreshPolicy().kinds(ProbeKind.LIVENESS));
        reg.registerFn("ready", () -> ComponentHealth.down("x"), new RefreshPolicy().kinds(ProbeKind.READINESS));
        HealthRegistry.Result live = reg.renderLiveness();
        assertEquals(200, live.code());
        assertTrue(live.json().contains("live"));
        assertTrue(!live.json().contains("ready"));
        assertEquals(503, reg.renderReadiness().code());
        assertEquals(200, reg.renderStartup().code());
    }

    @Test
    void degradedFailsReadinessNotLiveness() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("engine", () -> ComponentHealth.degraded("backpressure"),
                new RefreshPolicy().critical(true).kinds(ProbeKind.LIVENESS, ProbeKind.READINESS));
        assertEquals(503, reg.renderReadiness().code());
        assertEquals(200, reg.renderLiveness().code());
    }

    @Test
    void envExplicitPrefixGlob() {
        EnvProvider.MapEnv env = new EnvProvider.MapEnv()
                .with("KICKSTART_A", "1").with("APP_URL", "http://x").with("OTHER", "n");
        assertEquals(1, new EnvSection("d").keys(List.of("KICKSTART_A")).render(env).details().size());
        assertEquals(1, new EnvSection("d").prefix("KICKSTART_").render(env).details().size());
        assertEquals(1, new EnvSection("d").glob("*_URL").render(env).details().size());
    }

    @Test
    void redactionPolicies() {
        String v = "supersecretvalue";
        assertEquals("***", RedactionPolicy.MASK.apply(v));
        assertEquals("***alue", RedactionPolicy.LAST4.apply(v));
        String fp = RedactionPolicy.FINGERPRINT.apply(v);
        assertTrue(fp.startsWith("fp_") && fp.length() == 9);
        assertTrue(RedactionPolicy.HASH.apply(v).startsWith("fnv1a:"));
        assertNotEquals(RedactionPolicy.FINGERPRINT.apply("a"), RedactionPolicy.FINGERPRINT.apply("b"));
    }

    @Test
    void envRemapStripLowercase() {
        EnvProvider.MapEnv env = new EnvProvider.MapEnv()
                .with("KICKSTART_TARGET", "edge").with("KICKSTART_ENV", "prod");
        ComponentHealth c = new EnvSection("d")
                .prefix("KICKSTART_").stripPrefixInKey(true).lowercaseKeys(true)
                .remap("KICKSTART_TARGET", "where").render(env);
        assertEquals("edge", c.details().get("where"));
        assertTrue(c.details().containsKey("env"));
    }

    @Test
    void crossLanguageEnvSectionFixture() {
        EnvProvider.MapEnv env = new EnvProvider.MapEnv()
                .with("KICKSTART_ENV", "prod")
                .with("KICKSTART_VERSION", "1.2.3")
                .with("KICKSTART_TOKEN", "supersecret")
                .with("OTHER", "ignore");
        EnvSection section = new EnvSection("deploy")
                .prefix("KICKSTART_").stripPrefixInKey(true).lowercaseKeys(true).redactSecrets();
        assertEquals(
                "{\"status\":\"UP\",\"details\":{\"env\":\"prod\",\"token\":\"***\",\"version\":\"1.2.3\"}}",
                section.render(env).toJson());
    }

    @Test
    void crossLanguageReportFixture() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("db", () -> ComponentHealth.up().withDetail("ping", "ok"), new RefreshPolicy().critical(true));
        reg.registerFn("cache", () -> ComponentHealth.down("conn refused"), new RefreshPolicy().critical(false));
        HealthRegistry.Result r = reg.render();
        assertEquals(200, r.code());
        assertEquals(
                "{\"status\":\"WARN\",\"refreshed_at\":\"2026-06-18T00:00:00Z\",\"components\":{"
                        + "\"cache\":{\"status\":\"DOWN\",\"age_ms\":0,\"stale\":false,\"details\":{\"error\":\"conn refused\"}},"
                        + "\"db\":{\"status\":\"UP\",\"age_ms\":0,\"stale\":false,\"details\":{\"ping\":\"ok\"}}}}",
                r.json());
    }

    @Test
    void jsonEscaping() {
        ComponentHealth c = ComponentHealth.up().withDetail("msg", "a\"b\\c\nd\te");
        assertTrue(c.toJson().contains("a\\\"b\\\\c\\nd\\te"));
    }

    @Test
    void emptyRegistry() {
        HealthRegistry reg = fixedReg();
        HealthRegistry.Result r = reg.render();
        assertEquals(200, r.code());
        assertEquals("{\"status\":\"UP\",\"refreshed_at\":\"2026-06-18T00:00:00Z\"}", r.json());
    }

    @Test
    void stalenessFlag() {
        Clock.FixedClock clock = new Clock.FixedClock(1000, "2026-06-18T00:00:00Z");
        HealthRegistry reg = new HealthRegistry(HealthConfig.defaults().staleFactor(0.5), clock);
        reg.registerFn("x", ComponentHealth::up, new RefreshPolicy().intervalMs(100));
        reg.refreshNow();
        clock.set(1080);
        reg.refreshDue();
        String body = reg.render().json();
        assertTrue(body.contains("\"age_ms\":80"));
        assertTrue(body.contains("\"stale\":true"));
    }

    @Test
    void refreshPicksUpMutation() {
        AtomicBoolean down = new AtomicBoolean(false);
        HealthRegistry reg = fixedReg();
        reg.registerFn("flappy", () -> down.get() ? ComponentHealth.down("x") : ComponentHealth.up(),
                new RefreshPolicy().critical(true));
        assertEquals(HealthStatus.UP, reg.status());
        down.set(true);
        reg.refreshNow();
        assertEquals(HealthStatus.DOWN, reg.status());
    }

    @Test
    void statusChangeCallbackSyncDispatch() {
        List<String> seen = new ArrayList<>();
        HealthRegistry reg = fixedReg(HealthConfig.sync());
        reg.addListener(e -> seen.add(e.attr("scope") + ":" + e.attr("from") + ":" + e.attr("to")));
        AtomicBoolean down = new AtomicBoolean(false);
        reg.registerFn("api", () -> down.get() ? ComponentHealth.down("503") : ComponentHealth.up(),
                new RefreshPolicy().critical(true));
        reg.refreshNow();
        down.set(true);
        reg.refreshNow();
        assertTrue(seen.contains("overall:UP:DOWN"));
    }

    @Test
    void withSystemSectionsSmoke() {
        HealthRegistry reg = HealthRegistry.withSystemSections();
        HealthRegistry.Result r = reg.render();
        assertTrue(r.code() == 200 || r.code() == 503);
        assertTrue(r.json().contains("server"));
    }
}
