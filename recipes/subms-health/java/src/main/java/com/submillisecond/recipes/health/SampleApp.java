package com.submillisecond.recipes.health;

import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Sample app: a tour of {@code subms-health} over a trading gateway's
 * {@code /health}. Its readiness rolls up from a market-data feed, a pre-trade
 * risk check, and its exchange session, plus a non-critical order-book cache and
 * a redacted deploy section. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.health.SampleApp}
 *
 * <p>The Java port has no optional-feature classes (the Rust {@code datetime} /
 * {@code otel} Cargo features have no Java equivalent), so this is the base tour:
 * worst-wins rollup, probe-aware HTTP codes, non-critical demotion, and secret
 * redaction.
 */
public final class SampleApp {

    public static void main(String[] args) {
        gatewayReadiness();
    }

    static void gatewayReadiness() {
        System.out.println("== base: trading gateway readiness ==");

        AtomicBoolean sessionUp = new AtomicBoolean(true);
        AtomicBoolean riskTight = new AtomicBoolean(false);

        // Sync config: no background threads, we drive refreshNow() ourselves.
        HealthRegistry reg = new HealthRegistry(HealthConfig.sync());

        reg.registerFn("market-data-feed",
                () -> ComponentHealth.up().withDetail("msgs_per_sec", 184_000L),
                new RefreshPolicy().kinds(ProbeKind.READINESS));

        reg.registerFn("risk-check",
                () -> riskTight.get()
                        ? ComponentHealth.degraded("pre-trade limit 92% utilised")
                        : ComponentHealth.up().withDetail("limit_utilisation", "0.41"),
                new RefreshPolicy().kinds(ProbeKind.READINESS));

        reg.registerFn("exchange-session",
                () -> sessionUp.get()
                        ? ComponentHealth.up().withDetail("venue", "XLON")
                        : ComponentHealth.down("gateway logged out"),
                new RefreshPolicy().kinds(ProbeKind.LIVENESS, ProbeKind.READINESS));

        // Non-critical: a cold order-book cache never fails readiness, it is
        // demoted to WARN so the gateway keeps serving.
        reg.registerFn("orderbook-cache",
                () -> ComponentHealth.down("warm cache miss"),
                new RefreshPolicy().critical(false));

        EnvProvider.MapEnv env = new EnvProvider.MapEnv()
                .with("KICKSTART_ENV", "prod")
                .with("KICKSTART_REGION", "eu-west-1")
                .with("KICKSTART_API_TOKEN", "live-abc123-do-not-log");
        EnvSection deploy = new EnvSection("deploy")
                .prefix("KICKSTART_").stripPrefixInKey(true).lowercaseKeys(true).redactSecrets();
        reg.register(deploy.intoIndicator(env), new RefreshPolicy().critical(false));

        reg.start();

        // Steady state: the non-critical cache is DOWN but demoted to WARN, so the
        // gateway still serves (HTTP 200) even though overall is WARN.
        HealthRegistry.Result r = reg.render();
        System.out.println("  steady:   overall " + reg.status() + " -> HTTP " + r.code());
        if (reg.status() != HealthStatus.WARN) throw new AssertionError("cache demoted, not failed");
        if (r.code() != 200) throw new AssertionError("still serving");
        if (!r.json().contains("\"region\":\"eu-west-1\"")) throw new AssertionError("plain var kept");
        if (!r.json().contains("\"api_token\":\"***\"")) throw new AssertionError("secret masked");
        if (r.json().contains("live-abc123")) throw new AssertionError("raw secret never rendered");

        // Limits tighten: risk-check goes DEGRADED. Readiness 503s (pull from
        // rotation), liveness stays 200 - a degraded gateway is not restarted.
        riskTight.set(true);
        reg.refreshNow();
        int readyCode = reg.renderReadiness().code();
        int liveCode = reg.renderLiveness().code();
        System.out.println("  degraded: overall " + reg.status()
                + " -> ready " + readyCode + ", live " + liveCode);
        if (reg.status() != HealthStatus.DEGRADED) throw new AssertionError("worst-wins over WARN");
        if (readyCode != 503) throw new AssertionError("pulled from rotation");
        if (liveCode != 200) throw new AssertionError("not restarted");

        // The exchange session drops: DOWN wins worst-wins, and now liveness 503s.
        sessionUp.set(false);
        reg.refreshNow();
        liveCode = reg.renderLiveness().code();
        System.out.println("  down:     overall " + reg.status() + " -> live " + liveCode);
        if (reg.status() != HealthStatus.DOWN) throw new AssertionError("DOWN beats DEGRADED");
        if (liveCode != 503) throw new AssertionError("hard down restarts the pod");

        // Recovery: overall settles back to WARN (cache still cold), serving again.
        sessionUp.set(true);
        riskTight.set(false);
        reg.refreshNow();
        r = reg.render();
        System.out.println("  recover:  overall " + reg.status() + " -> HTTP " + r.code());
        if (r.code() != 200) throw new AssertionError("serving again");

        reg.stop();
    }
}
