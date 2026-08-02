package com.submillisecond.recipes.lsm;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureCategory;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.lsm.features.BlockKey;
import com.submillisecond.recipes.lsm.features.LeveledCompactionPlanner;
import com.submillisecond.recipes.lsm.features.LeveledManifest;
import com.submillisecond.recipes.lsm.features.LeveledRun;
import com.submillisecond.recipes.lsm.features.LruBlockCache;
import com.submillisecond.recipes.lsm.features.Lz4BlockCompressor;
import com.submillisecond.recipes.lsm.features.Snapshot;
import com.submillisecond.recipes.lsm.features.SnapshotManager;
import com.submillisecond.recipes.lsm.features.SnapshotManifest;
import com.submillisecond.recipes.lsm.features.TieredCompactionPlanner;
import com.submillisecond.recipes.lsm.features.TieredManifest;
import com.submillisecond.recipes.lsm.features.TieredRun;
import com.submillisecond.recipes.lsm.features.WriteAheadLog;
import com.submillisecond.recipes.lsm.features.ZstdBlockCompressor;

import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Consumer;
import java.util.function.IntConsumer;
import java.util.function.IntToLongFunction;
import java.util.function.Supplier;

/**
 * Feature classification bench, the Java mirror of
 * {@code rust/examples/perf_features.rs}. Each feature's representative op is
 * swept across three TREE SIZES, {@link SubMsFeatureManifest#classify} DECIDES
 * the category from the shape of that sweep, and the decision plus a measured
 * p99-by-stage is merge-written into {@code ../.subms/features/java.json}.
 *
 * <p>Live key count is the sweep axis because it is what sets everything an LSM
 * tree owns: the wal it has to replay, the entries a compaction has to rewrite,
 * the runs a snapshot pins, the blocks a cache has to hold. A per-op read is
 * size-independent and should read flat; anything that rewrites or rescans the
 * whole structure should climb with N.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE size and
 * ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted category is
 * an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q -o exec:java -Dexec.mainClass=com.submillisecond.recipes.lsm.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {

    /**
     * 8k / 64k / 512k live keys, a 64x span. The bottom is deliberately not 1k:
     * at a thousand entries a wal replay or a run merge is mostly fixed
     * per-call cost, which compresses the measured ratio and reads as flat.
     */
    private static final int[] SIZES = {8_192, 65_536, 524_288};
    private static final int CANON_N = SIZES[SIZES.length - 1];
    /** Per-op reps. Fixed across the sweep so a slope has one cause. */
    private static final int OPS = 20_000;
    /** Timed repeats for a whole-structure op, far too slow to run OPS times. */
    private static final int BULK_REPS = 32;
    /**
     * Bulk warmup is TIME-BOXED rather than a fixed rep count. A fixed count
     * leaves the first sweep point running interpreted while every later point
     * reuses the compiled method, which reads as a curve that falls with size.
     */
    private static final long BULK_WARM_NANOS = 300_000_000L;
    private static final int BULK_WARM_MAX_REPS = 5_000;
    /**
     * Per-op warm, also time-boxed, for the same reason and one more. A fixed
     * 20_000 reps is 24 ms of a block codec, which is not enough to settle an
     * allocator that has just had a few hundred megabytes of compaction
     * templates freed under it: the Rust port read lz4 at 2200 -> 1500 -> 1100
     * ns across a sweep whose axis it does not even touch, and on the previous
     * run the same artifact landed on zstd instead. The op is capped as well as
     * timed so a 200 ns planner call does not spend the full budget.
     */
    private static final long KEYED_WARM_NANOS = 300_000_000L;
    private static final int KEYED_WARM_MAX_REPS = 200_000;

    /** One SSTable data block. Held CONSTANT across the sweep - see the codecs. */
    private static final int BLOCK_BYTES = 4096;
    /** Entries per run in the compaction manifests. 128 runs at the top size. */
    private static final int ENTRIES_PER_RUN = 4_096;
    /** Live keys behind one cached 4KB block, so the cache scales with the tree. */
    private static final int KEYS_PER_BLOCK = 8;
    /** Live keys per on-disk run, so a snapshot pins a manifest that grows with N. */
    private static final int KEYS_PER_SSTABLE = 4_096;
    /**
     * Big enough that the base tree ends up with ~20 runs rather than ~1300; a
     * read walking 1300 blooms measures the flush threshold, not the read path.
     */
    private static final int FLUSH_BYTES = 1_000_000;

    private static final String VALUE = "value-payload-bytes-24ch";
    private static final byte[] VALUE_BYTES = VALUE.getBytes(StandardCharsets.UTF_8);

    /**
     * Zero-padded so key order matches insertion order and a run's key range is
     * a contiguous interval - what leveled compaction's overlap selection
     * assumes.
     */
    private static String key(int i) {
        String s = Integer.toString(i);
        StringBuilder sb = new StringBuilder(10).append('k');
        for (int p = s.length(); p < 9; p++) {
            sb.append('0');
        }
        return sb.append(s).toString();
    }

    /** Spreads probes over the whole key space without a live rng in the timed loop. */
    private static int probe(int i, int n) {
        return (int) ((i * 2_654_435_761L) % n);
    }

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        Path tmp = Files.createTempDirectory("subms-lsm-features-");
        try {
            // The baseline: a base `get` against a real tree at the canonical
            // size. Every feature is classified against the cost of the read it
            // decorates.
            long baseP50;
            try (LsmTree tree = new LsmTree(tmp.resolve("base"), FLUSH_BYTES)) {
                for (int i = 0; i < CANON_N; i++) {
                    tree.put(key(i), VALUE);
                }
                tree.flush();
                String[] probes = new String[OPS];
                for (int i = 0; i < OPS; i++) {
                    probes[i] = key(probe(i, CANON_N));
                }
                baseP50 = keyed(i -> {
                    try {
                        consume(tree.get(probes[i]));
                    } catch (IOException e) {
                        throw new UncheckedIOException(e);
                    }
                }, true);
            }
            System.err.println("base get p50: " + baseP50 + "ns (" + CANON_N + " live keys)");

            wal(manifest, baseP50, tmp);
            tiered(manifest, baseP50);
            leveled(manifest, baseP50);
            snapshot(manifest, baseP50);
            lz4(manifest, baseP50);
            zstd(manifest, baseP50);
            blockCache(manifest, baseP50);
        } finally {
            deleteRecursive(tmp);
        }

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- wal: durable append, whole-log replay ----------

    /** Writes {@code n} put records and returns the log path. */
    private static Path walOf(Path dir, int n) {
        Path path = dir.resolve("replay-" + n + ".wal");
        try {
            Files.deleteIfExists(path);
            try (WriteAheadLog wal = new WriteAheadLog(path)) {
                for (int i = 0; i < n; i++) {
                    wal.logPut(key(i), VALUE_BYTES);
                }
                wal.sync();
            }
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        return path;
    }

    private static long replayOnce(Path path, boolean median) {
        return bulk(() -> {
            try {
                consume(WriteAheadLog.replay(path).size());
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }, median);
    }

    private static void wal(SubMsFeatureManifest manifest, long baseP50, Path dir) {
        // Swept on `replay`, not on `logPut`. The append is one buffered write
        // per record and is flat by construction; recovery - reading and
        // CRC-verifying every record written since the last flush - is what the
        // wal EXISTS for, and it is the part that grows with the tree.
        long[][] sweep = sweep("wal/replay", n -> {
            Path p = walOf(dir, n);
            long out = replayOnce(p, true);
            deleteQuietly(p);
            return out;
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Path replayPath = walOf(dir, CANON_N);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("replay", replayOnce(replayPath, false));
        deleteQuietly(replayPath);

        // A scratch log, so the append measurement does not inflate the one above.
        Path appendPath = dir.resolve("append.wal");
        deleteQuietly(appendPath);
        String[] keys = new String[OPS];
        for (int i = 0; i < OPS; i++) {
            keys[i] = key(i);
        }
        try (WriteAheadLog wal = new WriteAheadLog(appendPath)) {
            p99.put("log_put", keyed(i -> {
                try {
                    wal.logPut(keys[i], VALUE_BYTES);
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
            }, false));
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        deleteQuietly(appendPath);
        // `sync` is deliberately absent from both the sweep and the stage table.
        // fsync is a device property - tens of us on battery-backed NVMe,
        // single-digit ms on this laptop tier - so a number for it would move
        // with the hardware under a column the reader reads as the cost of the
        // code, and sweeping it would dress a constant storage-stack cost as a
        // scaling result.
        manifest.setFeature("wal", dec.category(), p99, dec.reason());
    }

    // ---------- tiered-compaction: merge every run at a level into one ----------

    /**
     * The runs are rebuilt into a fresh manifest per rep but the run objects
     * themselves are SHARED: the planner clears the level list and reads the
     * entries, never mutating a run, so one template serves every rep. The Rust
     * port has to deep-clone instead because its merge moves the runs out of
     * the level. Either way the rebuild sits outside the timed region.
     */
    private static List<TieredRun> tieredRuns(int n) {
        int runCount = (n + ENTRIES_PER_RUN - 1) / ENTRIES_PER_RUN;
        List<TieredRun> runs = new ArrayList<>(runCount);
        for (int r = 0; r < runCount; r++) {
            List<TieredRun.Entry> entries = new ArrayList<>(ENTRIES_PER_RUN);
            for (int j = 0; j < ENTRIES_PER_RUN; j++) {
                entries.add(new TieredRun.Entry(key(r * ENTRIES_PER_RUN + j), VALUE_BYTES));
            }
            runs.add(new TieredRun(r, entries));
        }
        return runs;
    }

    private static TieredManifest tieredManifest(List<TieredRun> runs) {
        TieredManifest m = new TieredManifest();
        for (TieredRun r : runs) {
            m.push(0, r);
        }
        return m;
    }

    private static void tiered(SubMsFeatureManifest manifest, long baseP50) {
        TieredCompactionPlanner planner = new TieredCompactionPlanner(2);
        // Swept on `merge`, not on `pickLevel`. Picking a level is a scan of the
        // per-level run counts and is O(levels); the merge is the whole point of
        // the feature and rewrites every entry at the level.
        long[][] sweep = sweep("tiered-compaction/merge", n -> {
            List<TieredRun> template = tieredRuns(n);
            return bulkEach(() -> tieredManifest(template), m -> planner.merge(m, 0, 9_999), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        List<TieredRun> template = tieredRuns(CANON_N);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("merge",
                bulkEach(() -> tieredManifest(template), m -> planner.merge(m, 0, 9_999), false));
        TieredManifest planned = tieredManifest(template);
        p99.put("plan", keyed(i -> consume(planner.pickLevel(planned)), false));
        manifest.setFeature("tiered-compaction", dec.category(), p99, dec.reason());
    }

    // ---------- leveled-compaction: merge L0 into the overlapping L1 runs ----------

    /**
     * L0 and L1 interleave over the same key space (even indices vs odd), so
     * every L1 run overlaps the L0 range and the compaction picks all of them
     * up. Disjoint halves would leave L1 untouched and the merge would only ever
     * rewrite half the tree.
     */
    private static List<LeveledRun> leveledRuns(int n, int parity) {
        int perLevel = n / 2;
        int runCount = (perLevel + ENTRIES_PER_RUN - 1) / ENTRIES_PER_RUN;
        List<LeveledRun> runs = new ArrayList<>(runCount);
        for (int r = 0; r < runCount; r++) {
            List<TieredRun.Entry> entries = new ArrayList<>(ENTRIES_PER_RUN);
            for (int j = 0; j < ENTRIES_PER_RUN; j++) {
                int idx = 2 * (r * ENTRIES_PER_RUN + j) + parity;
                entries.add(new TieredRun.Entry(key(idx), VALUE_BYTES));
            }
            runs.add(new LeveledRun(r * 2L + parity, entries));
        }
        return runs;
    }

    private static LeveledManifest leveledManifest(List<LeveledRun> l0, List<LeveledRun> l1) {
        LeveledManifest m = new LeveledManifest();
        for (LeveledRun r : l0) {
            m.push(0, r);
        }
        for (LeveledRun r : l1) {
            m.push(1, r);
        }
        return m;
    }

    private static void leveled(SubMsFeatureManifest manifest, long baseP50) {
        LeveledCompactionPlanner planner = new LeveledCompactionPlanner(64_000, 10, 4);
        // Swept on `compact`, not on `pickLevel`. The budget scan is O(runs);
        // the compaction rewrites every entry it touches.
        long[][] sweep = sweep("leveled-compaction/compact", n -> {
            List<LeveledRun> l0 = leveledRuns(n, 0);
            List<LeveledRun> l1 = leveledRuns(n, 1);
            return bulkEach(() -> leveledManifest(l0, l1),
                    m -> planner.compact(m, 0, 9_999), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        List<LeveledRun> l0 = leveledRuns(CANON_N, 0);
        List<LeveledRun> l1 = leveledRuns(CANON_N, 1);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("compact", bulkEach(() -> leveledManifest(l0, l1),
                m -> planner.compact(m, 0, 9_999), false));
        LeveledManifest planned = leveledManifest(l0, l1);
        p99.put("plan", keyed(i -> consume(planner.pickLevel(planned)), false));
        manifest.setFeature("leveled-compaction", dec.category(), p99, dec.reason());
    }

    // ---------- snapshot: pinned point-in-time view of the run manifest ----------

    private static SnapshotManager manager(int n) {
        int runs = Math.max(1, n / KEYS_PER_SSTABLE);
        List<Long> ids = new ArrayList<>(runs);
        for (long i = 0; i < runs; i++) {
            ids.add(i);
        }
        return new SnapshotManager(new SnapshotManifest(ids));
    }

    private static void snapshot(SubMsFeatureManifest manifest, long baseP50) {
        // The manifest under test grows with the tree - 2 run ids at the bottom
        // size, 128 at the top - which is what makes a flat result mean
        // something. Taking a snapshot grabs an immutable reference and bumps an
        // id under a short lock, so it should not care, and the sweep is how
        // that is shown rather than asserted.
        long[][] sweep = sweep("snapshot/snapshot", n -> {
            SnapshotManager mgr = manager(n);
            return keyed(i -> consume(mgr.snapshot()), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        SnapshotManager mgr = manager(CANON_N);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("snapshot", keyed(i -> consume(mgr.snapshot()), false));
        // The read side: resolve a key against a held view by walking its pinned
        // run ids newest-first, the order the tree's own read path uses.
        Snapshot held = mgr.snapshot();
        List<Long> ids = held.sstableIds();
        long[] targets = new long[OPS];
        for (int i = 0; i < OPS; i++) {
            targets[i] = probe(i, Math.max(1, ids.size()) * 2);
        }
        p99.put("get_on_snapshot", keyed(i -> {
            long t = targets[i];
            boolean found = false;
            for (int idx = ids.size() - 1; idx >= 0; idx--) {
                if (ids.get(idx) == t) {
                    found = true;
                    break;
                }
            }
            consume(found);
        }, false));
        manifest.setFeature("snapshot", dec.category(), p99, dec.reason());
    }

    // ---------- lz4 / zstd: SSTable block compression ----------

    /**
     * A representative ~4KB SSTable data block: repeating record-shaped text so
     * the compressors have realistic-but-not-degenerate redundancy.
     */
    private static byte[] representativeBlock() {
        byte[] pattern = "key-0000042\0present\0value-payload-bytes-for-block|"
                .getBytes(StandardCharsets.UTF_8);
        byte[] out = new byte[BLOCK_BYTES];
        for (int i = 0; i < BLOCK_BYTES; i++) {
            out[i] = pattern[i % pattern.length];
        }
        return out;
    }

    private static void lz4(SubMsFeatureManifest manifest, long baseP50) {
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        // The block is held at BLOCK_BYTES at EVERY sweep point. Compression
        // cost tracks the bytes handed to the codec, so growing the block with
        // the tree would publish a payload sweep dressed as a tree-size sweep -
        // and an LSM block size is a configuration constant, not a function of
        // how many keys are live. The flat curve is the finding: a bigger tree
        // is more blocks at the same per-block cost, not a more expensive block.
        byte[] block = representativeBlock();
        long[][] sweep = sweep("lz4/compress", n -> keyed(i -> consume(c.compress(block)), true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        byte[] encoded = c.compress(block);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("compress_block", keyed(i -> consume(c.compress(block)), false));
        p99.put("decompress_block", keyed(i -> {
            try {
                consume(c.decompress(encoded));
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }, false));
        manifest.setFeature("lz4", dec.category(), p99, dec.reason());
    }

    private static void zstd(SubMsFeatureManifest manifest, long baseP50) throws IOException {
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        byte[] block = representativeBlock();
        long[][] sweep = sweep("zstd/compress", n -> keyed(i -> {
            try {
                consume(c.compress(block));
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }, true));
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        byte[] encoded = c.compress(block);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("compress_block", keyed(i -> {
            try {
                consume(c.compress(block));
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }, false));
        p99.put("decompress_block", keyed(i -> {
            try {
                consume(c.decompress(encoded));
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        }, false));
        manifest.setFeature("zstd", dec.category(), p99, dec.reason());
    }

    // ---------- block-cache-integration: read-path block cache ----------

    /**
     * Capacity scales with the tree and the cache is filled to it, so the
     * occupied fraction is the same at every sweep point. A fixed 1024 slots
     * would hold the map at one size while claiming to sweep the tree. One
     * shared payload keeps 64k cached blocks in memory instead of 256 MB of
     * identical bytes; the cache stores the reference either way.
     */
    private static LruBlockCache filled(int n) {
        int cap = Math.max(64, n / KEYS_PER_BLOCK);
        LruBlockCache cache = new LruBlockCache(cap);
        byte[] block = representativeBlock();
        for (long i = 0; i < cap; i++) {
            cache.put(new BlockKey(i % 8, i * BLOCK_BYTES), block);
        }
        return cache;
    }

    private static BlockKey[] hitKeys(int cap) {
        BlockKey[] keys = new BlockKey[OPS];
        for (int i = 0; i < OPS; i++) {
            long k = probe(i, cap);
            keys[i] = new BlockKey(k % 8, k * BLOCK_BYTES);
        }
        return keys;
    }

    private static void blockCache(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sweep = sweep("block-cache-integration/get_cached", n -> {
            int cap = Math.max(64, n / KEYS_PER_BLOCK);
            LruBlockCache cache = filled(n);
            BlockKey[] keys = hitKeys(cap);
            return keyed(i -> consume(cache.get(keys[i])), true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        int cap = Math.max(64, CANON_N / KEYS_PER_BLOCK);
        LruBlockCache cache = filled(CANON_N);
        BlockKey[] hits = hitKeys(cap);
        BlockKey[] misses = new BlockKey[OPS];
        for (int i = 0; i < OPS; i++) {
            misses[i] = new BlockKey(999, probe(i, cap));
        }
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("get_cached", keyed(i -> consume(cache.get(hits[i])), false));
        p99.put("get_miss", keyed(i -> consume(cache.get(misses[i])), false));
        manifest.setFeature("block-cache-integration", dec.category(), p99, dec.reason());
    }

    // ---------- harness plumbing ----------

    /**
     * Sweeps and PRINTS the curve. A ratio-compressed or non-monotonic curve
     * classifies flat and the rows are the only place it shows.
     */
    private static long[][] sweep(String label, IntToLongFunction at) {
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < SIZES.length; i++) {
            rows[i][0] = SIZES[i];
            rows[i][1] = at.applyAsLong(SIZES[i]);
            sb.append('(').append(rows[i][0]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    /** A per-op measurement, warmed over the same index range it then times. */
    private static long keyed(IntConsumer op, boolean median) {
        // Warm to C2 first. An unwarmed JIT costs most on the FIRST measured
        // size, which the sweep reads as a cost that FALLS with N - the opposite
        // of the structural signal, and just as wrong.
        long deadline = System.nanoTime() + KEYED_WARM_NANOS;
        for (int i = 0; i < KEYED_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(i % OPS);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("lsm-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        for (int i = 0; i < OPS; i++) {
            int idx = i;
            st.time(() -> op.accept(idx));
        }
        return stat(h, median);
    }

    /**
     * A whole-structure op that leaves its input intact, so one setup serves
     * every rep. Measured cold, a bulk op lands its first-touch cost on
     * whichever sweep point runs first, which reads as a curve that FALLS with
     * size - the opposite of the structural signal.
     */
    private static long bulk(Runnable op, boolean median) {
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.run();
        }
        SubMsPerfHarness h = new SubMsPerfHarness("lsm-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", BULK_REPS);
        for (int i = 0; i < BULK_REPS; i++) {
            st.time(op);
        }
        return stat(h, median);
    }

    /**
     * A whole-structure op that CONSUMES its input. Both compaction entry points
     * take the runs out of the level they compact, so a second rep would merge
     * an empty level and the curve would read flat. {@code setup} rebuilds the
     * input before each rep, OUTSIDE the timed region - the alternative,
     * rebuilding inside the closure, publishes the manifest build as if it were
     * the merge.
     */
    private static <T> long bulkEach(Supplier<T> setup, Consumer<T> op, boolean median) {
        long deadline = System.nanoTime() + BULK_WARM_NANOS;
        for (int i = 0; i < BULK_WARM_MAX_REPS && System.nanoTime() < deadline; i++) {
            op.accept(setup.get());
        }
        SubMsPerfHarness h = new SubMsPerfHarness("lsm-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", BULK_REPS);
        for (int i = 0; i < BULK_REPS; i++) {
            final T input = setup.get();
            st.time(() -> op.accept(input));
        }
        return stat(h, median);
    }

    private static long stat(SubMsPerfHarness h, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }

    /** Keeps the JIT from eliding a pure-read result. */
    private static void consume(Object o) {
        SINK = o;
    }

    private static volatile Object SINK;

    private static void deleteQuietly(Path p) {
        try {
            Files.deleteIfExists(p);
        } catch (IOException ignored) {
            // best-effort cleanup
        }
    }

    private static void deleteRecursive(Path dir) {
        try {
            if (!Files.exists(dir)) return;
            try (var walk = Files.walk(dir)) {
                walk.sorted((a, b) -> b.getNameCount() - a.getNameCount())
                    .forEach(PerfFeaturesMain::deleteQuietly);
            }
        } catch (IOException ignored) {
            // best-effort cleanup
        }
    }

    private PerfFeaturesMain() {}
}
