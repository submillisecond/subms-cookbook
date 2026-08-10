package com.submillisecond.recipes.timer;

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
 * Storage-growth capture for the timer wheel: under continuous schedule+tick
 * churn, the pending-timer index ({@code idToSlot}) must reach a bounded steady
 * state and stay there. If {@code tick} fired timers but forgot to evict their
 * ids, the index would climb every round - a slow leak a percentile can't see.
 * This measures the index size round over round and gates it flat.
 *
 * <p>Emits the stable subms growth JSON on stdout. Mirror of the Rust
 * {@code examples/growth_main.rs}, reading the same stdin key=value config, so
 * the two curves are directly comparable.
 *
 * <pre>
 * printf 'rounds=50\nnum_slots=256\nops_per_round=20000\n' \
 *   | java -cp target/classes:... com.submillisecond.recipes.timer.GrowthMain
 * </pre>
 */
public final class GrowthMain {

    /**
     * Rough per-pending-entry heap cost: the HashMap node (32) with its boxed
     * Long id (24) and boxed Integer slot (16), plus the slot's Entry (32), its
     * slot-list array element (4), and the boxed Long payload (24).
     *
     * <p>132 against the Rust port's 48 for the same pending timer: Rust stores
     * the id and slot inline in a hash table and the payload inline in the slot,
     * where the JVM boxes all three and threads them through a linked node. Both
     * ports still hold exactly the live set, so the plateau verdict agrees even
     * though the byte counts do not.
     */
    private static final long ENTRY_BYTES = 32 + 24 + 16 + 32 + 4 + 24;

    private GrowthMain() {}

    private static final class WheelChurn implements SubMsGrowthRecipe {
        private final TimerWheel<Long> wheel;
        private final int numSlots;
        private final int rounds;
        private final int opsPerRound;
        private long seq;

        WheelChurn(int numSlots, int rounds, int opsPerRound) {
            this.wheel = new TimerWheel<>(numSlots);
            this.numSlots = numSlots;
            this.rounds = rounds;
            this.opsPerRound = opsPerRound;
        }

        @Override public String name() {
            return "subms-timer-wheel";
        }

        @Override public String opName() {
            return "schedule";
        }

        @Override public int rounds() {
            return rounds;
        }

        @Override public int opsPerRound() {
            return opsPerRound;
        }

        @Override public void op(int round, int i) {
            // Schedule one timer somewhere in the next rotation, then advance the
            // hand one tick (firing anything now due). Over many ops the in-flight
            // set reaches a steady size; a leaking tick would let it grow without
            // bound.
            long delay = (i % (numSlots - 1)) + 1;
            wheel.schedule(delay, seq);
            seq++;
            wheel.tick();
        }

        @Override public long memoryBytes() {
            return (long) wheel.pending() * ENTRY_BYTES;
        }

        @Override public long liveBytes() {
            // The genuinely-in-flight timers are the live set; a correct wheel holds
            // exactly those, so resident == live.
            return (long) wheel.pending() * ENTRY_BYTES;
        }

        @Override public Map<String, Long> structures() {
            return Map.of("pending", (long) wheel.pending());
        }

        @Override public SubMsGrowth.GrowthClass expectedClass() {
            return SubMsGrowth.GrowthClass.PLATEAU_BOUNDED;
        }

        @Override public double expectedBound() {
            // The pending index must plateau at its steady size, not climb round over
            // round - climbing would mean fired ids are never evicted.
            return 1.5;
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

        var recipe = new WheelChurn(
                Math.max(4, parseInt(cfg, "num_slots", 256)),
                parseInt(cfg, "rounds", 50),
                parseInt(cfg, "ops_per_round", 20_000));

        Writer out = new OutputStreamWriter(System.out, StandardCharsets.UTF_8);
        SubMsGrowth.growthToJson(SubMsGrowth.grow(recipe, "java"), out);
        out.flush();
    }
}
