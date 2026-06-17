package com.submillisecond.recipes.tsplan;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/**
 * Drives a representative subms-ts-plan workload: build a 10-stage plan, the
 * shape a real prune-&gt;decode-&gt;scan-&gt;quantile query lowers to, then certify
 * and verify it. Stages mirror the Rust recipe: {@code certify}, {@code verify}.
 */
public final class PlanRecipe implements SubMsRecipe {

    @Override
    public String name() {
        return "subms-ts-plan";
    }

    private static TsPlan build() {
        TsPlan p = new TsPlan().withOverhead(50_000);
        for (int i = 0; i < 10; i++) {
            p = p.then("subms-ts", "range_min", 900 + (long) i * 10);
        }
        return p;
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int rounds = params.entries();

        long sink = 0;
        SubMsPerfHarness.Stage sCert = h.stage("certify", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < rounds; i++) {
            TsPlan plan = build();
            long t0 = SubMsTimer.nanosNow();
            TsLatencyCertificate cert = plan.certify("ci-dedicated", 0);
            sCert.record(SubMsTimer.nanosNow() - t0);
            sink += cert.totalP99Ns();
        }

        TsLatencyCertificate cert = build().certify("ci-dedicated", 0);
        SubMsPerfHarness.Stage sVerify = h.stage("verify", rounds).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < rounds; i++) {
            long t0 = SubMsTimer.nanosNow();
            boolean ok = cert.verify();
            sVerify.record(SubMsTimer.nanosNow() - t0);
            sink += ok ? 1 : 0;
        }
        BLACK_HOLE = sink;

        h.meta("plan_stages", "10");
        h.meta("subms.workload.feature", "latency-certificate");
    }

    static volatile long BLACK_HOLE;
}
