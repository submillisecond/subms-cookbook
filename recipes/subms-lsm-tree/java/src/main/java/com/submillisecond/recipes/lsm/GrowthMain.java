package com.submillisecond.recipes.lsm;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.UncheckedIOException;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.HashMap;
import java.util.Map;
import java.util.stream.Stream;

import com.submillisecond.perf.SubMsGrowth;
import com.submillisecond.perf.SubMsGrowthRecipe;

/**
 * Storage-growth capture for the LSM tree: overwrite the SAME key set every
 * round and watch on-disk bytes + SSTable count against a FLAT live set.
 * Without compaction each flush would leave a fresh run while the previous
 * rounds' now-dead versions pile up unreclaimed - the classic
 * write-amplification leak. With compaction ENABLED
 * ({@link LsmTree#setCompactionTrigger(int)}, plus a merge at each round
 * boundary), the tree reclaims those dead versions and on-disk stays within a
 * small multiple of the live set. This bench runs the fixed, compacting config
 * and gates the amplification bounded - green, where the un-compacted base tree
 * would breach.
 *
 * <p>Emits the stable subms growth JSON on stdout. Mirror of the Rust
 * {@code examples/growth_main.rs}, reading the same stdin key=value config, so
 * the two curves are directly comparable.
 *
 * <p>The footprint here is bytes on disk, not an object-layout number, so the
 * two ports do not diverge the way a heap-accounted recipe does: the SSTable
 * layout is the same in both, and at rounds=8 / keys=2000 / value_bytes=256
 * both write 550,528 bytes and hold 1 run per round after the merge, against
 * 530,000 live bytes (2000 x (256 + 9)) - amplification 1.0387x in each. The
 * SSTable count and the amplification ratio are the cross-port checks; the raw
 * byte count agreeing is a property of the shared file format, not something
 * the verdict depends on.
 *
 * <pre>
 * printf 'rounds=50\nkeys=2000\nvalue_bytes=256\nflush_threshold_bytes=65536\ncompaction_trigger=4\n' \
 *   | java -cp target/classes:... com.submillisecond.recipes.lsm.GrowthMain
 * </pre>
 */
public final class GrowthMain {

    /**
     * With compaction the tree must keep on-disk within a small multiple of live
     * data even under overwrite churn. A little slack over 1x for the runs that
     * accumulate between compactions.
     */
    private static final double AMPLIFICATION_CEILING = 3.0;

    private GrowthMain() {}

    private static final class OverwriteChurn implements SubMsGrowthRecipe {
        private final LsmTree lsm;
        private final Path dir;
        private final int rounds;
        private final int keys;
        private final int valueBytes;
        private final String value;

        OverwriteChurn(Path dir, int rounds, int keys, int valueBytes, int flushThreshold,
                int compactionTrigger) throws IOException {
            deleteRecursively(dir);
            Files.createDirectories(dir);
            this.lsm = new LsmTree(dir, flushThreshold);
            // The fix: auto-compact once runs accumulate, so overwritten versions
            // are reclaimed instead of piling up.
            this.lsm.setCompactionTrigger(compactionTrigger);
            this.dir = dir;
            this.rounds = rounds;
            this.keys = keys;
            this.valueBytes = valueBytes;
            this.value = "x".repeat(valueBytes);
        }

        private static String key(int i) {
            return String.format("k%08d", i);
        }

        @Override public String name() {
            return "subms-lsm-tree";
        }

        @Override public String opName() {
            return "put";
        }

        @Override public int rounds() {
            return rounds;
        }

        @Override public int opsPerRound() {
            return keys;
        }

        @Override public void op(int round, int i) {
            // Overwrite the SAME key each round: the live set is flat, so every byte
            // compaction fails to reclaim shows up as pure amplification.
            try {
                lsm.put(key(i), value);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }

        @Override public void endRound(int round) {
            // Flush the memtable, then compact so on-disk reflects only live data at
            // the round boundary - the reclaim the fix provides.
            try {
                lsm.flush();
                lsm.compact();
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }

        @Override public long diskBytes() {
            // Sum every file under the data dir (SSTables + their bloom trailers).
            try (Stream<Path> files = Files.list(dir)) {
                return files.filter(Files::isRegularFile).mapToLong(p -> {
                    try {
                        return Files.size(p);
                    } catch (IOException e) {
                        return 0L;
                    }
                }).sum();
            } catch (IOException e) {
                return 0L;
            }
        }

        @Override public long liveBytes() {
            // The logical working set: `keys` distinct entries, each one value plus
            // its key. Flat across rounds because we only ever overwrite.
            return (long) keys * (valueBytes + key(0).length());
        }

        @Override public Map<String, Long> structures() {
            return Map.of("sstables", (long) lsm.sstableCount());
        }

        @Override public SubMsGrowth.GrowthClass expectedClass() {
            return SubMsGrowth.GrowthClass.AMPLIFICATION_BOUNDED;
        }

        @Override public double expectedBound() {
            return AMPLIFICATION_CEILING;
        }

        void close() throws IOException {
            lsm.close();
        }
    }

    private static void deleteRecursively(Path dir) throws IOException {
        if (!Files.exists(dir)) {
            return;
        }
        try (Stream<Path> walk = Files.walk(dir)) {
            for (Path p : walk.sorted(Comparator.reverseOrder()).toList()) {
                Files.deleteIfExists(p);
            }
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

        Path dir = Path.of(System.getProperty("java.io.tmpdir"),
                "lsm-growth-" + ProcessHandle.current().pid());
        var recipe = new OverwriteChurn(
                dir,
                parseInt(cfg, "rounds", 50),
                parseInt(cfg, "keys", 2000),
                parseInt(cfg, "value_bytes", 256),
                parseInt(cfg, "flush_threshold_bytes", 64 * 1024),
                parseInt(cfg, "compaction_trigger", 4));

        var report = SubMsGrowth.grow(recipe, "java");
        recipe.close();

        Writer out = new OutputStreamWriter(System.out, StandardCharsets.UTF_8);
        SubMsGrowth.growthToJson(report, out);
        out.flush();
    }
}
