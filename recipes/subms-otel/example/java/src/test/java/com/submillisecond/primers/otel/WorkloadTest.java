package com.submillisecond.primers.otel;

import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class WorkloadTest {

    /** Recording observer; captures every onRecord ctx for assertion. */
    private static final class CapturingObserver implements SubMsObserver {
        final List<SubMsObservationCtx> records = new ArrayList<>();
        @Override
        public void onRecord(SubMsObservationCtx ctx, long ns) {
            records.add(ctx);
        }
    }

    @Test
    @DisplayName("workload sets the standard recipe-identity meta keys on the harness")
    void declaresRecipeIdentityMeta() {
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java");
        Workload.runWorkload(h, 64);
        assertEquals(Workload.RECIPE_SLUG, h.meta().get("subms.recipe.slug"));
        assertEquals(Workload.RECIPE_CATEGORY, h.meta().get("subms.recipe.category"));
        assertEquals("64", h.inputs().get("entries"));
    }

    @Test
    @DisplayName("workload emits the three expected stages, each tagged HOT_PATH")
    void stagesAreHotPath() {
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java");
        Workload.runWorkload(h, 32);
        Set<String> seen = new HashSet<>();
        for (SubMsPerfHarness.Stage s : h.stagesInOrder()) {
            seen.add(s.name());
            assertEquals(SubMsStageKind.HOT_PATH, s.kind(),
                    "stage " + s.name() + " must be marked HOT_PATH");
        }
        assertEquals(Set.of("put", "get_hit", "get_miss"), seen);
    }

    @Test
    @DisplayName("a recording observer sees the right ctx attributes on each record")
    void observerSeesExpectedCtx() {
        CapturingObserver obs = new CapturingObserver();
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java")
                .withObserver(obs);

        Workload.runWorkload(h, 16);

        // 3 stages * 16 ops = 48 records.
        assertEquals(48, obs.records.size());
        for (SubMsObservationCtx ctx : obs.records) {
            assertEquals("subms-primer-otel", ctx.workload());
            assertEquals("java", ctx.lang());
            assertEquals(SubMsStageKind.HOT_PATH, ctx.stageKind());
            assertTrue(
                    ctx.stage().equals("put")
                            || ctx.stage().equals("get_hit")
                            || ctx.stage().equals("get_miss"),
                    "unexpected stage " + ctx.stage());
        }
    }

    @Test
    @DisplayName("records arrive in stage-registration order: put -> get_hit -> get_miss")
    void recordsArriveInStageOrder() {
        CapturingObserver obs = new CapturingObserver();
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java")
                .withObserver(obs);
        Workload.runWorkload(h, 8);

        // The first 8 must be put, the next 8 get_hit, then get_miss.
        for (int i = 0; i < 8; i++) assertEquals("put", obs.records.get(i).stage());
        for (int i = 8; i < 16; i++) assertEquals("get_hit", obs.records.get(i).stage());
        for (int i = 16; i < 24; i++) assertEquals("get_miss", obs.records.get(i).stage());
    }

    @Test
    @DisplayName("zero / negative entries reject; no-observer path stays a no-op")
    void illegalEntriesAndNoObserver() {
        SubMsPerfHarness h = new SubMsPerfHarness("subms-primer-otel", "java");
        assertThrows(IllegalArgumentException.class, () -> Workload.runWorkload(h, 0));
        assertThrows(IllegalArgumentException.class, () -> Workload.runWorkload(h, -5));

        // No observer registered: workload still runs cleanly; harness simply has no observer to call.
        SubMsPerfHarness clean = new SubMsPerfHarness("subms-primer-otel", "java");
        Workload.runWorkload(clean, 4);
        assertFalse(clean.stagesInOrder().isEmpty());
    }
}
