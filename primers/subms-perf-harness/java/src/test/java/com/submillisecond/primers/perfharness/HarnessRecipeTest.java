package com.submillisecond.primers.perfharness;

import java.util.HashSet;
import java.util.List;
import java.util.Set;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageSummary;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Behavioural tests for the recipe wiring plus the embedded
 * SubMillisecondBench-style p99 assertion. The bench runs at a deliberately
 * small {@code entries} so the test suite stays fast; the assertion still
 * fires and would catch a regression that pushes p99 past 1 ms.
 */
final class HarnessRecipeTest {

    private static final long ONE_MS_NS = 1_000_000L;

    @Test
    @DisplayName("recipe name matches the published workload identifier")
    void recipeName() {
        assertEquals("subms-perf-harness", new HarnessRecipe().name());
    }

    @Test
    @DisplayName("runBench populates all three stages with the expected sample counts")
    void runBenchProducesStages() {
        SubMsBenchParams params = new SubMsBenchParams(2_000, 500, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new HarnessRecipe(), params);
        SubMsBenchSummary s = SubMsBench.summarizeLean(h);

        assertEquals("java", s.lang());
        assertEquals("subms-perf-harness", s.workload());

        SubMsStageSummary put     = s.stage("put").orElseThrow();
        SubMsStageSummary getHit  = s.stage("get_hit").orElseThrow();
        SubMsStageSummary getMiss = s.stage("get_miss").orElseThrow();
        assertEquals(params.entries(), put.count());
        assertEquals(params.entries(), getHit.count());
        assertEquals(params.entries(), getMiss.count());
    }

    @Test
    @DisplayName("inputs map carries entries/warmup/seed verbatim")
    void inputsReflectParams() {
        SubMsBenchParams params = new SubMsBenchParams(1_500, 250, 42L);
        SubMsPerfHarness h = SubMsBench.runBench(new HarnessRecipe(), params);
        SubMsBenchSummary s = SubMsBench.summarize(h);

        assertEquals("1500", s.inputs().get("entries"));
        assertEquals("250",  s.inputs().get("warmup"));
        assertEquals("42",   s.inputs().get("seed"));
    }

    @Test
    @DisplayName("meta map carries the recipe-set hardware/structure tags")
    void metaCarriesRecipeTags() {
        SubMsBenchParams params = new SubMsBenchParams(1_000, 200, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new HarnessRecipe(), params);
        SubMsBenchSummary s = SubMsBench.summarize(h);
        assertEquals("laptop", s.meta().get("hardware_tier"));
        assertEquals("open-addressing-linear-probe", s.meta().get("structure"));
        assertNotNull(s.meta().get("load_factor_target"));
    }

    @Test
    @DisplayName("get_hit and get_miss are computed against disjoint key streams")
    void hitAndMissKeysAreDisjoint() {
        long[] hits  = HarnessRecipe.deterministicKeys(5_000, 0L);
        long[] misses = HarnessRecipe.deterministicKeys(5_000, 0L ^ 0xDEAD_BEEF_CAFE_F00DL);
        Set<Long> hitSet = new HashSet<>();
        for (long k : hits) hitSet.add(k);
        int collisions = 0;
        for (long k : misses) if (hitSet.contains(k)) collisions++;
        // SplitMix64 with a 64-bit salt over 5k samples each should
        // statistically never collide; if this trips, the streams aren't
        // actually independent.
        assertEquals(0, collisions,
                "hit and miss key streams collided " + collisions + " times");
    }

    @Test
    @DisplayName("deterministicKeys is deterministic for a fixed seed")
    void deterministicKeysAreStable() {
        long[] a = HarnessRecipe.deterministicKeys(1_000, 7L);
        long[] b = HarnessRecipe.deterministicKeys(1_000, 7L);
        assertEquals(1_000, a.length);
        for (int i = 0; i < a.length; i++) assertEquals(a[i], b[i]);
        long[] c = HarnessRecipe.deterministicKeys(1_000, 8L);
        assertNotEquals(a[0], c[0]);   // different seed, different stream
    }

    @Test
    @DisplayName("sub-millisecond p99 holds for put / get_hit / get_miss")
    void subMillisecondBench() {
        SubMsBenchParams params = new SubMsBenchParams(5_000, 1_000, 0L);
        SubMsBenchSummary s = SubMsBench.summarizeLean(
                SubMsBench.runBench(new HarnessRecipe(), params));

        SubMsBench.printSummary(s, System.out);

        SubMsBench.assertP99Under(s, List.of(
                new SubMsBench.Assertion("put",      ONE_MS_NS),
                new SubMsBench.Assertion("get_hit",  ONE_MS_NS),
                new SubMsBench.Assertion("get_miss", ONE_MS_NS)));
    }

    @Test
    @DisplayName("assertP99Under throws AssertionError when a stage is missing")
    void assertionThrowsOnMissingStage() {
        SubMsBenchParams params = new SubMsBenchParams(500, 100, 0L);
        SubMsBenchSummary s = SubMsBench.summarizeLean(
                SubMsBench.runBench(new HarnessRecipe(), params));
        assertThrows(AssertionError.class,
                () -> SubMsBench.assertP99Under(s, List.of(
                        new SubMsBench.Assertion("does_not_exist", ONE_MS_NS))));
    }

    @Test
    @DisplayName("summary JSON contains the workload identifier and stage names")
    void summaryJsonContainsExpectedShape() throws Exception {
        SubMsBenchParams params = new SubMsBenchParams(500, 100, 0L);
        SubMsPerfHarness h = SubMsBench.runBench(new HarnessRecipe(), params);
        SubMsBenchSummary s = SubMsBench.summarize(h);

        java.io.StringWriter sw = new java.io.StringWriter();
        SubMsBench.summaryToJson(s, sw);
        String json = sw.toString();

        assertTrue(json.contains("\"workload\":\"subms-perf-harness\""), json);
        assertTrue(json.contains("\"put\""), json);
        assertTrue(json.contains("\"get_hit\""), json);
        assertTrue(json.contains("\"get_miss\""), json);
        assertTrue(json.contains("\"p99_ns\""), json);
    }
}
