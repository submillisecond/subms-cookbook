package com.submillisecond.recipes.blockcache;

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
 * Storage-growth capture for the block cache: stream far more distinct keys
 * through a fixed-capacity LRU than it can hold, and confirm the resident set
 * stays pinned at the capacity. Eviction is what bounds memory here - this
 * proves it, rather than the cache being a map that quietly grows.
 *
 * <p>Emits the stable subms growth JSON on stdout. Mirror of the Rust
 * {@code examples/growth_main.rs}, reading the same stdin key=value config, so
 * the two curves are directly comparable.
 *
 * <pre>
 * printf 'rounds=50\ncapacity=1024\ninserts_per_round=20000\n' \
 *   | java -cp target/classes:... com.submillisecond.recipes.blockcache.GrowthMain
 * </pre>
 */
public final class GrowthMain {

    private static final int VALUE_BYTES = 256;
    /** Rough per-entry heap cost: value + key (long) + slot/index bookkeeping. */
    private static final long ENTRY_BYTES = VALUE_BYTES + 8 + 32;

    private GrowthMain() {}

    private static final class CacheChurn implements SubMsGrowthRecipe {
        private final BlockCache<Long, byte[]> cache;
        private final int capacity;
        private final int rounds;
        private final int insertsPerRound;
        private final byte[] value;
        private long next;

        CacheChurn(int capacity, int rounds, int insertsPerRound) {
            this.cache = new BlockCache<>(capacity);
            this.capacity = capacity;
            this.rounds = rounds;
            this.insertsPerRound = insertsPerRound;
            this.value = new byte[VALUE_BYTES];
        }

        @Override public String name() {
            return "subms-block-cache";
        }

        @Override public String opName() {
            return "put";
        }

        @Override public int rounds() {
            return rounds;
        }

        @Override public int opsPerRound() {
            return insertsPerRound;
        }

        @Override public void op(int round, int i) {
            // A fresh distinct key every op: nothing is ever re-hit, so a naive
            // unbounded map would grow to millions - the LRU must evict to stay
            // flat.
            cache.put(next, value.clone());
            next++;
        }

        @Override public long memoryBytes() {
            return (long) cache.size() * ENTRY_BYTES;
        }

        @Override public long liveBytes() {
            // A cache holds only live entries, so resident == live: amplification
            // 1x is the healthy shape.
            return (long) cache.size() * ENTRY_BYTES;
        }

        @Override public Map<String, Long> structures() {
            return Map.of("entries", (long) cache.size());
        }

        @Override public SubMsGrowth.GrowthClass expectedClass() {
            return SubMsGrowth.GrowthClass.BOUNDED;
        }

        @Override public double expectedBound() {
            // Resident memory must never exceed the capacity's worth, no matter
            // how many distinct keys stream through. 5% slack for the bookkeeping
            // estimate.
            return (double) (capacity * ENTRY_BYTES) * 1.05;
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

        var recipe = new CacheChurn(
                parseInt(cfg, "capacity", 1024),
                parseInt(cfg, "rounds", 50),
                parseInt(cfg, "inserts_per_round", 20_000));

        Writer out = new OutputStreamWriter(System.out, StandardCharsets.UTF_8);
        SubMsGrowth.growthToJson(SubMsGrowth.grow(recipe, "java"), out);
        out.flush();
    }
}
