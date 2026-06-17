package com.submillisecond.recipes.tssql;

import java.util.List;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;

/**
 * Asserts the recipe's p99 contract. {@code parse} is genuinely sub-ms (a
 * char-cursor lexer + recursive-descent walk over a moderate query). {@code
 * query} is throughput-contracted, NOT a per-op sub-ms primitive: each sample
 * parses + lowers + runs a full group-by-aggregate over a 4,096-row frame with a
 * WHERE filter. On a laptop tier the p99 rides the JVM's GC / allocation tail
 * (the lazy filter materialises the frame's row axis and the group-by allocates
 * a sub-frame per group), so a generous guard is the honest contract; the number
 * to read is throughput, in perf/java.json. The Rust sibling holds parse under
 * 1 ms and query in the low single-digit milliseconds.
 */
public final class SubmillisecondBench {
    private static final long PARSE_GUARD_NS = 1_000_000L;
    private static final long QUERY_GUARD_NS = 40_000_000L;

    public static void main(String[] args) {
        SubMsBenchParams params = new SubMsBenchParams(5_000, 1_000, 7L);
        SubMsPerfHarness h = SubMsBench.runBench(new SqlRecipe(), params);
        SubMsBench.assertP99Under(h, List.of(
                new SubMsBench.Assertion("parse", PARSE_GUARD_NS),
                new SubMsBench.Assertion("query", QUERY_GUARD_NS)));
        System.out.println("OK (parse p99 under 1 ms, query p99 under 40 ms guard)");
    }
}
