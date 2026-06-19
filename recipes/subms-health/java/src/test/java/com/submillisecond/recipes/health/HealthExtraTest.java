package com.submillisecond.recipes.health;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

import com.submillisecond.recipes.events.DispatchMode;
import org.junit.jupiter.api.Test;

class HealthExtraTest {

    private static HealthRegistry fixedReg() {
        return new HealthRegistry(HealthConfig.defaults(), new Clock.FixedClock(1000, "2026-06-18T00:00:00Z"));
    }

    @Test
    void healthEndpointDispatchesAllProbes() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("a", ComponentHealth::up, new RefreshPolicy().kinds(ProbeKind.values()));
        assertEquals(200, HealthEndpoint.handle(reg, null).code());
        assertEquals(200, HealthEndpoint.handle(reg, ProbeKind.LIVENESS).code());
        assertEquals(200, HealthEndpoint.handle(reg, ProbeKind.READINESS).code());
        assertEquals(200, HealthEndpoint.handle(reg, ProbeKind.STARTUP).code());
    }

    @Test
    void healthConfigBuilders() {
        HealthConfig c = HealthConfig.defaults()
                .mode(HealthConfig.RefreshMode.SYNC)
                .tickMs(250)
                .dispatch(DispatchMode.SYNC)
                .staleFactor(2.0);
        assertEquals(HealthConfig.RefreshMode.SYNC, c.mode());
        assertEquals(250, c.tickMs());
        assertEquals(DispatchMode.SYNC, c.dispatch());
        assertEquals(2.0, c.staleFactor());
        HealthConfig s = HealthConfig.sync();
        assertEquals(HealthConfig.RefreshMode.SYNC, s.mode());
        assertEquals(DispatchMode.SYNC, s.dispatch());
    }

    @Test
    void componentVariantsAndNestedJson() {
        assertEquals(HealthStatus.UNKNOWN, ComponentHealth.unknown().status());
        assertEquals(HealthStatus.DEGRADED, ComponentHealth.degraded("slow").status());
        ComponentHealth parent = ComponentHealth.up()
                .withDetail("n", 7)
                .withDetail("ok", true)
                .withDetail("big", 9_000_000_000L)
                .withSubcomponent("child", ComponentHealth.down("x"));
        String json = parent.toJson();
        assertTrue(json.contains("\"n\":7"));
        assertTrue(json.contains("\"ok\":true"));
        assertTrue(json.contains("\"big\":9000000000"));
        assertTrue(json.contains("\"components\":{\"child\":{\"status\":\"DOWN\""));
    }

    @Test
    void envSectionOptions() {
        EnvProvider.MapEnv env = new EnvProvider.MapEnv()
                .with("KICKSTART_EMPTY", "")
                .with("KICKSTART_TOKEN", "abcd1234")
                .with("APP_HOST", "h");
        // includeEmpty pulls in the set-but-empty var; status() sets the node status.
        ComponentHealth c = new EnvSection("d")
                .prefix("KICKSTART_").includeEmpty(true).status(HealthStatus.WARN)
                .redact("KICKSTART_TOKEN", RedactionPolicy.FINGERPRINT)
                .render(env);
        assertEquals(HealthStatus.WARN, c.status());
        assertTrue(c.details().containsKey("KICKSTART_EMPTY"));
        assertTrue(((String) c.details().get("KICKSTART_TOKEN")).startsWith("fp_"));
        // suffix glob + single key
        assertEquals(1, new EnvSection("d").glob("APP_*").render(env).details().size());
        assertEquals(1, new EnvSection("d").key("APP_HOST").render(env).details().size());
        // redactSubstring with a policy
        ComponentHealth h = new EnvSection("d").prefix("KICKSTART_")
                .redactSubstring("TOKEN", RedactionPolicy.HASH).render(env);
        assertTrue(((String) h.details().get("KICKSTART_TOKEN")).startsWith("fnv1a:"));
    }

    @Test
    void registerSectionAndRenderVariants() {
        HealthRegistry reg = fixedReg();
        EnvProvider.MapEnv env = new EnvProvider.MapEnv().with("KICKSTART_ENV", "prod");
        reg.register(new EnvSection("deploy").prefix("KICKSTART_").intoIndicator(env),
                new RefreshPolicy().kinds(ProbeKind.values()).critical(false));
        assertTrue(reg.renderLiveness().json().contains("deploy"));
        assertTrue(reg.renderReadiness().json().contains("deploy"));
        assertTrue(reg.renderStartup().json().contains("deploy"));
        reg.refreshDue();
        assertNotNull(reg.render().json());
    }

    @Test
    void startupDownReturns503() {
        HealthRegistry reg = fixedReg();
        reg.registerFn("boot", () -> ComponentHealth.down("starting"),
                new RefreshPolicy().critical(true).kinds(ProbeKind.STARTUP));
        assertEquals(503, reg.renderStartup().code());
    }

    @Test
    void asyncStartStopRefreshes() throws InterruptedException {
        HealthRegistry reg = new HealthRegistry(HealthConfig.defaults().tickMs(5));
        reg.registerFn("a", ComponentHealth::up, new RefreshPolicy().intervalMs(0));
        reg.start();
        for (int i = 0; i < 20; i++) {
            assertEquals(200, reg.render().code());
        }
        Thread.sleep(30);
        reg.stop();
        assertEquals(HealthStatus.UP, reg.status());
    }

    @Test
    void componentLevelChangeEventEmitted() {
        List<String> seen = new ArrayList<>();
        HealthRegistry reg = new HealthRegistry(HealthConfig.sync(), new Clock.FixedClock(1000, "2026-06-18T00:00:00Z"));
        reg.addListener(e -> seen.add(e.attr("scope") + ":" + e.attr("from") + ":" + e.attr("to")));
        AtomicBoolean down = new AtomicBoolean(false);
        reg.registerFn("cache", () -> down.get() ? ComponentHealth.down("x") : ComponentHealth.up(),
                new RefreshPolicy().critical(false));
        reg.refreshNow();
        down.set(true);
        reg.refreshNow();
        assertTrue(seen.contains("cache:UP:DOWN"));
        assertTrue(seen.contains("overall:UP:WARN"));
    }

    @Test
    void serverIndicatorReports() {
        ComponentHealth c = new ServerIndicator().check();
        assertEquals(HealthStatus.UP, c.status());
        assertTrue(c.details().containsKey("pid"));
        assertTrue(c.details().containsKey("hostname"));
        assertTrue(c.details().containsKey("uptime_ms"));
    }

    @Test
    void systemClockAndEnvAreUsable() {
        Clock clock = new Clock.SystemClock();
        assertTrue(clock.nowMs() > 0);
        assertNotNull(clock.nowRfc3339());
        EnvProvider env = new EnvProvider.SystemEnv();
        assertNotNull(env.keys());
    }
}
