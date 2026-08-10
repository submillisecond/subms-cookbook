package com.submillisecond.recipes.arena;

import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.Writer;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

import com.submillisecond.perf.SubMsGrowth;
import com.submillisecond.perf.SubMsGrowthRecipe;

/**
 * Storage-growth capture for the bump arena: run many alloc-then-{@code reset}
 * sessions and confirm resident memory returns to the same steady level every
 * round. A bump allocator has zero per-object overhead, so resident tracks the
 * live allocations exactly (amplification ~1x), and {@code reset} reclaims all
 * of it - the arena never accretes across sessions.
 *
 * <p>Emits the stable subms growth JSON on stdout. Mirror of the Rust
 * {@code examples/growth_main.rs}, reading the same stdin key=value config, so
 * the two curves are directly comparable.
 *
 * <p>The bytes here do NOT diverge from the Rust port. The footprint is a
 * cursor offset into the backing buffer, not a count of objects:
 * {@code allocs_per_round} allocations of 8 bytes at alignment 8 from a cursor
 * starting at 0 leave {@code used() == 8 * allocs} in both ports (160,000 at
 * the default 20,000; measured equal round for round at 5,000 allocations
 * against the Rust example). The JVM's 16-byte array header
 * and the arena shell sit outside that accounting exactly as Rust's Vec header
 * does, so both curves report the same number.
 *
 * <pre>
 * printf 'rounds=50\nallocs_per_round=20000\n' \
 *   | java -cp target/classes:... com.submillisecond.recipes.arena.GrowthMain
 * </pre>
 */
public final class GrowthMain {

    private GrowthMain() {}

    private static final class ArenaChurn implements SubMsGrowthRecipe {
        private final BumpArena arena;
        private final ByteBuffer view;
        private final int rounds;
        private final int allocsPerRound;

        ArenaChurn(int capacity, int rounds, int allocsPerRound) {
            this.arena = new BumpArena(capacity);
            // The Rust op is alloc_copy, which allocates AND stores the value;
            // the view keeps the store in the timed path so the two ops match.
            this.view = ByteBuffer.wrap(arena.bytes()).order(ByteOrder.nativeOrder());
            this.rounds = rounds;
            this.allocsPerRound = allocsPerRound;
        }

        @Override public String name() {
            return "subms-arena-allocator";
        }

        @Override public String opName() {
            return "alloc";
        }

        @Override public int rounds() {
            return rounds;
        }

        @Override public int opsPerRound() {
            return allocsPerRound;
        }

        @Override public void op(int round, int i) {
            // Start each round's session fresh: reset reclaims the whole buffer,
            // then we bump-allocate the round's objects into it.
            if (i == 0) {
                arena.reset();
            }
            int off = arena.allocate(8, 8);
            view.putLong(off, i);
        }

        @Override public long memoryBytes() {
            // Resident = bytes currently handed out of the buffer (measured at the
            // round's peak, before the next round's reset).
            return arena.used();
        }

        @Override public long liveBytes() {
            // Every allocated byte is live until reset, so resident == live: a bump
            // arena wastes nothing (amplification 1x).
            return arena.used();
        }

        @Override public Map<String, Long> structures() {
            return Map.of("live_allocs", (long) allocsPerRound);
        }

        @Override public SubMsGrowth.GrowthClass expectedClass() {
            return SubMsGrowth.GrowthClass.PLATEAU_BOUNDED;
        }

        @Override public double expectedBound() {
            // Resident memory must return to the same steady level every round - it
            // must not climb, which would mean reset is leaking the buffer.
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

        int rounds = parseInt(cfg, "rounds", 50);
        int allocsPerRound = parseInt(cfg, "allocs_per_round", 20_000);

        // Buffer sized to hold one round's allocations (long + alignment) with
        // headroom - the base arena is fixed-capacity and throws rather than
        // growing.
        int capacity = allocsPerRound * 16 + 4096;
        var recipe = new ArenaChurn(capacity, rounds, allocsPerRound);

        Writer out = new OutputStreamWriter(System.out, StandardCharsets.UTF_8);
        SubMsGrowth.growthToJson(SubMsGrowth.grow(recipe, "java"), out);
        out.flush();
    }
}
