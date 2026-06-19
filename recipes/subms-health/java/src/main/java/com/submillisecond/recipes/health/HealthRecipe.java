package com.submillisecond.recipes.health;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stages: register / report / render_json. Mirrors the Rust recipe. */
public final class HealthRecipe implements SubMsRecipe {
    private static final int INDICATORS = 16;
    private static final int ENV_VARS_PER_SECTION = 8;

    private static HealthRegistry buildRegistry() {
        HealthRegistry reg = new HealthRegistry(HealthConfig.sync());
        for (int i = 0; i < INDICATORS - 2; i++) {
            boolean healthy = i % 5 != 0;
            reg.registerFn("dep-" + i,
                    () -> healthy
                            ? ComponentHealth.up().withDetail("ping", "ok").withDetail("rtt_us", 42)
                            : ComponentHealth.degraded("slow upstream").withDetail("rtt_us", 9_000),
                    new RefreshPolicy().intervalMs(1_000));
        }
        for (int s = 0; s < 2; s++) {
            EnvProvider.MapEnv env = new EnvProvider.MapEnv();
            for (int v = 0; v < ENV_VARS_PER_SECTION; v++) {
                env.with("KICKSTART_VAR" + s + "_" + v, "value-" + s + "-" + v);
            }
            EnvSection section = new EnvSection("deploy-" + s)
                    .prefix("KICKSTART_").stripPrefixInKey(true).lowercaseKeys(true).redactSecrets();
            reg.register(section.intoIndicator(env), new RefreshPolicy().intervalMs(60_000).critical(false));
        }
        return reg;
    }

    @Override
    public String name() {
        return "subms-health";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();

        SubMsPerfHarness.Stage sReg = h.stage("register", entries).withKind(SubMsStageKind.BATCH_OP);
        long sink = 0;
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            HealthRegistry reg = buildRegistry();
            sReg.record(SubMsTimer.nanosNow() - t0);
            sink += reg.hashCode();
        }

        HealthRegistry reg = buildRegistry();
        SubMsPerfHarness.Stage sReport = h.stage("report", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            reg.refreshNow();
            sReport.record(SubMsTimer.nanosNow() - t0);
        }

        reg.refreshNow();
        SubMsPerfHarness.Stage sRender = h.stage("render_json", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            long t0 = SubMsTimer.nanosNow();
            HealthRegistry.Result r = reg.render();
            sRender.record(SubMsTimer.nanosNow() - t0);
            sink += r.json().length();
        }

        if (sink == Long.MIN_VALUE) {
            throw new IllegalStateException("unreachable");
        }
        h.meta("indicators", Integer.toString(INDICATORS));
        h.meta("env_vars_per_section", Integer.toString(ENV_VARS_PER_SECTION));
        h.meta("subms.workload.feature", "cached-snapshot-render");
    }
}
