package com.submillisecond.primers.otel;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;

/**
 * Toy workload over {@link TinyMap}: insert N entries, look each one up
 * (every probe a hit), then probe N keys that are guaranteed to miss. Three
 * stages emitted to the harness: {@code put}, {@code get_hit}, {@code get_miss}.
 *
 * <p>Every stage is annotated {@link SubMsStageKind#HOT_PATH} so the
 * downstream {@code OtelObserver} sees the canonical per-request kind. The
 * harness's standard recipe-identity meta keys are set so a registered
 * observer's exported OTEL metric carries {@code subms.recipe.slug} +
 * {@code subms.recipe.category}, matching the attribute set every other
 * cookbook recipe emits.
 */
public final class Workload {

    /** Default per-stage sample count for the primer. */
    public static final int DEFAULT_ENTRIES = 5_000;

    /** Recipe slug carried on every exported OTEL data point. */
    public static final String RECIPE_SLUG = "subms-primer-otel";

    /** Recipe category - tooling-tier because this is a primer over the observer hook. */
    public static final String RECIPE_CATEGORY = "tooling";

    private Workload() {}

    /** Run the workload with the default entry count. */
    public static void runWorkload(SubMsPerfHarness h) {
        runWorkload(h, DEFAULT_ENTRIES);
    }

    /**
     * Run {@code entries} put / get_hit / get_miss ops against a fresh
     * {@link TinyMap}, declaring the standard recipe-identity meta on the
     * harness so observer-exported metrics carry the full attribute set.
     */
    public static void runWorkload(SubMsPerfHarness h, int entries) {
        if (entries <= 0) throw new IllegalArgumentException("entries must be > 0");

        h.input("entries", Integer.toString(entries));
        h.meta("subms.recipe.slug", RECIPE_SLUG);
        h.meta("subms.recipe.category", RECIPE_CATEGORY);

        TinyMap map = new TinyMap(entries * 2);

        SubMsPerfHarness.Stage put = h.stage("put", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            final long k = i + 1;
            final long v = k * 31L;
            put.time(() -> map.put(k, v));
        }

        SubMsPerfHarness.Stage getHit = h.stage("get_hit", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            final long k = i + 1;
            getHit.time(() -> map.get(k));
        }

        SubMsPerfHarness.Stage getMiss = h.stage("get_miss", entries).withKind(SubMsStageKind.HOT_PATH);
        for (int i = 0; i < entries; i++) {
            // Keys above the inserted range; guaranteed misses without the EMPTY-marker collision.
            final long k = entries + i + 1L;
            getMiss.time(() -> map.get(k));
        }
    }
}
