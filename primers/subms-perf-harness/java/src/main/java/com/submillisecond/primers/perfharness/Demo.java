package com.submillisecond.primers.perfharness;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * The harness API in its rawest form - no {@code SubMsRecipe}, no params
 * object, no {@code runBench}. Useful for one-off measurements: drop a
 * harness in next to the code under test, time a couple of named stages,
 * print the table.
 *
 * <p>This is the shape the primer writeup shows first. {@link PerfMain}
 * + {@link HarnessRecipe} are the "production" wiring; this is the
 * scratch-paper version.
 *
 * <pre>
 *   java -cp target/classes com.submillisecond.primers.perfharness.Demo
 * </pre>
 */
public final class Demo {
    private Demo() {}

    private static final int ENTRIES = 10_000;

    public static void main(String[] args) {
        SubMsPerfHarness h = new SubMsPerfHarness("demo", "java");
        h.input("entries", Integer.toString(ENTRIES));

        TinyMap map = new TinyMap(ENTRIES * 2);
        long[] keys = HarnessRecipe.deterministicKeys(ENTRIES, 0);

        // Manual stage. No recipe, no params object - just time a runnable.
        SubMsPerfHarness.Stage put = h.stage("put", ENTRIES);
        for (int i = 0; i < ENTRIES; i++) {
            final int idx = i;
            put.time(() -> map.put(keys[idx], idx));
        }

        // Explicit ns recording - the same effect, useful when the work
        // crosses several scopes and a lambda would be awkward.
        SubMsPerfHarness.Stage get = h.stage("get", ENTRIES);
        for (int i = 0; i < ENTRIES; i++) {
            long t0 = System.nanoTime();
            map.get(keys[i]);
            get.record(System.nanoTime() - t0);
        }

        // The single most useful one-liner: summarise + print in one call.
        SubMsBench.printSummary(h, System.out);
    }
}
