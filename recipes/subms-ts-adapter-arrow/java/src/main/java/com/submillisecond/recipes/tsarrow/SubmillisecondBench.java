package com.submillisecond.recipes.tsarrow;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import java.util.List;

/**
 * Asserts the recipe's per-op sub-ms claim. Each stage converts a whole
 * 4,096-point series to / from an Arrow root - {@code to_batch} and
 * {@code from_batch}. The columnar build is a bulk buffer fill, so both clear
 * p99 &lt; 1 ms. IPC framing is a separate, reported number in perf/java.json.
 *
 * <p>Run with {@code --add-opens=java.base/java.nio=ALL-UNNAMED} (Arrow's
 * off-heap buffers need reflective nio access on JDK 21).
 */
public final class SubmillisecondBench {
    private static final long TO_GUARD_NS = 1_000_000L;
    private static final long FROM_GUARD_NS = 1_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(1_000, 200, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new ArrowRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("to_batch", TO_GUARD_NS),
                new SubMsBench.Assertion("from_batch", FROM_GUARD_NS)));
        System.out.println("OK (to_batch + from_batch p99 under 1 ms)");
    }
}
