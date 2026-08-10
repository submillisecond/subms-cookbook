package com.submillisecond.recipes.ratelimit;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

import com.submillisecond.perf.SubMsGrowth;
import com.submillisecond.perf.SubMsGrowthRecipe;

/**
 * Storage-growth capture for the rate limiter: hammer it with acquisitions and
 * confirm the footprint never moves. The limiter is a single GCRA bucket - one
 * atomic of state plus a couple of constants - with no per-key map, so its
 * memory is O(1) no matter how many callers or how long it runs. A compact
 * verdict, not a curve.
 *
 * <p>Emits the stable subms growth JSON on stdout. Mirror of the Rust
 * {@code examples/growth_main.rs}, reading the same stdin key=value config, so
 * the two curves are directly comparable.
 *
 * <pre>
 * printf 'rounds=20\nacquires_per_round=100000\n' \
 *   | java -cp target/classes:... com.submillisecond.recipes.ratelimit.GrowthMain
 * </pre>
 */
public final class GrowthMain {

    /**
     * Object header (12) + the AtomicLong reference (4) + three longs (24), which
     * lands on 40 with no padding under compressed oops.
     */
    private static final long LIMITER_SHELL_BYTES = 40;
    /** The AtomicLong is its own object: header (12) + long (8), padded to 24. */
    private static final long ATOMIC_BYTES = 24;
    /**
     * 64 against the Rust port's 40 for the same structure: Rust inlines its
     * AtomicU64 into the struct, the JVM allocates it separately and pads. The
     * verdict is what the two ports must agree on, and both are O(1) bounded -
     * the absolute byte counts are object-layout artifacts and are expected to
     * differ.
     */
    private static final long FOOTPRINT_BYTES = LIMITER_SHELL_BYTES + ATOMIC_BYTES;

    private GrowthMain() {}

    private static final class LimiterChurn implements SubMsGrowthRecipe {
        private final RateLimiter limiter;
        private final int rounds;
        private final int acquiresPerRound;
        private long granted;

        LimiterChurn(int rounds, int acquiresPerRound) {
            // High rate + burst so most acquisitions succeed; the footprint is the
            // point, not the grant ratio.
            this.limiter = new RateLimiter(10_000_000.0, 1_000_000);
            this.rounds = rounds;
            this.acquiresPerRound = acquiresPerRound;
        }

        @Override public String name() {
            return "subms-rate-limiter";
        }

        @Override public String opName() {
            return "acquire";
        }

        @Override public int rounds() {
            return rounds;
        }

        @Override public int opsPerRound() {
            return acquiresPerRound;
        }

        @Override public void op(int round, int i) {
            if (limiter.tryAcquire()) {
                granted++;
            }
        }

        @Override public long memoryBytes() {
            return FOOTPRINT_BYTES;
        }

        @Override public long liveBytes() {
            return FOOTPRINT_BYTES;
        }

        @Override public Map<String, Long> structures() {
            return Map.of("granted", granted);
        }

        @Override public SubMsGrowth.GrowthClass expectedClass() {
            return SubMsGrowth.GrowthClass.BOUNDED;
        }

        @Override public double expectedBound() {
            return (double) FOOTPRINT_BYTES * 1.01;
        }

        @Override public boolean compact() {
            return true;
        }
    }

    private static int parseInt(Map<String, String> cfg, String key, int fallback) {
        String v = cfg.get(key);
        if (v == null) {
            return fallback;
        }
        try {
            return Integer.parseInt(v.trim());
        } catch (NumberFormatException e) {
            return fallback;
        }
    }

    public static void main(String[] args) throws Exception {
        Map<String, String> cfg = new HashMap<>();
        try (BufferedReader in =
                new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                int eq = line.indexOf('=');
                if (eq > 0) {
                    cfg.put(line.substring(0, eq).trim(), line.substring(eq + 1).trim());
                }
            }
        }

        var recipe = new LimiterChurn(
                parseInt(cfg, "rounds", 20),
                parseInt(cfg, "acquires_per_round", 100_000));

        Writer out = new OutputStreamWriter(System.out, StandardCharsets.UTF_8);
        SubMsGrowth.growthToJson(SubMsGrowth.grow(recipe, "java"), out);
        out.flush();
    }
}
