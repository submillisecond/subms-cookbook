package com.submillisecond.recipes.health;

import java.util.concurrent.atomic.AtomicBoolean;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        HealthRegistry reg = new HealthRegistry(HealthConfig.sync());
        reg.registerFn("market-data-feed",
                () -> ComponentHealth.up().withDetail("msgs_per_sec", 184_000L),
                new RefreshPolicy());
        reg.start();                              // sync mode: warms the cache, no thread

        HealthRegistry.Result r = reg.render();   // cache read - never probes
        assertEquals(200, r.code());
        assertEquals(HealthStatus.UP, reg.status());
        // quickstart:end
    }

    private static HealthRegistry gateway(AtomicBoolean sessionUp, AtomicBoolean riskTight) {
        HealthRegistry reg = new HealthRegistry(HealthConfig.sync());
        reg.registerFn("market-data-feed", ComponentHealth::up,
                new RefreshPolicy().kinds(ProbeKind.READINESS));
        reg.registerFn("risk-check",
                () -> riskTight.get()
                        ? ComponentHealth.degraded("pre-trade limit 92% utilised")
                        : ComponentHealth.up(),
                new RefreshPolicy().kinds(ProbeKind.READINESS));
        reg.registerFn("exchange-session",
                () -> sessionUp.get() ? ComponentHealth.up() : ComponentHealth.down("gateway logged out"),
                new RefreshPolicy().kinds(ProbeKind.LIVENESS, ProbeKind.READINESS));
        reg.registerFn("orderbook-cache",
                () -> ComponentHealth.down("warm cache miss"),
                new RefreshPolicy().critical(false));
        reg.start();
        return reg;
    }

    @Test
    void nonCriticalFailureIsDemoted() {
        HealthRegistry reg = gateway(new AtomicBoolean(true), new AtomicBoolean(false));
        HealthRegistry.Result r = reg.render();
        assertEquals(HealthStatus.WARN, reg.status(), "cache demoted to WARN");
        assertEquals(200, r.code(), "still serving");
    }

    @Test
    void degradedIsProbeAware() {
        AtomicBoolean sessionUp = new AtomicBoolean(true);
        AtomicBoolean riskTight = new AtomicBoolean(false);
        HealthRegistry reg = gateway(sessionUp, riskTight);
        riskTight.set(true);
        reg.refreshNow();
        assertEquals(HealthStatus.DEGRADED, reg.status(), "worst-wins over WARN");
        assertEquals(503, reg.renderReadiness().code(), "pulled from rotation");
        assertEquals(200, reg.renderLiveness().code(), "not restarted");
    }

    @Test
    void downWinsAndRestarts() {
        AtomicBoolean sessionUp = new AtomicBoolean(true);
        HealthRegistry reg = gateway(sessionUp, new AtomicBoolean(true));
        sessionUp.set(false);
        reg.refreshNow();
        assertEquals(HealthStatus.DOWN, reg.status(), "DOWN beats DEGRADED");
        assertEquals(503, reg.renderLiveness().code(), "hard down restarts the pod");
    }

    @Test
    void deploySectionMasksSecrets() {
        HealthRegistry reg = new HealthRegistry(HealthConfig.sync());
        EnvProvider.MapEnv env = new EnvProvider.MapEnv()
                .with("KICKSTART_REGION", "eu-west-1")
                .with("KICKSTART_API_TOKEN", "live-abc123-do-not-log");
        EnvSection deploy = new EnvSection("deploy")
                .prefix("KICKSTART_").stripPrefixInKey(true).lowercaseKeys(true).redactSecrets();
        reg.register(deploy.intoIndicator(env), new RefreshPolicy().critical(false));
        reg.start();

        String json = reg.render().json();
        assertTrue(json.contains("\"region\":\"eu-west-1\""), "plain var kept");
        assertTrue(json.contains("\"api_token\":\"***\""), "secret masked");
        assertFalse(json.contains("live-abc123"), "raw secret never rendered");
    }
}
