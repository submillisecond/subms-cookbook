package com.submillisecond.recipes.lsm;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
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
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * One stage block per opt-in feature, measuring the per-op cost the cookbook
 * page reports. The base LSM read/write path lives in {@link PerfMain}; this
 * covers only the feature modules, with the SAME stage names as the Rust bench
 * so the cookbook FeaturePicker columns line up across languages. JSON
 * contract goes to stdout.
 *
 * <p>The Rust side gates each variant behind a Cargo feature; Java ships them
 * all on the classpath, so every stage is always emitted here.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt; com.submillisecond.recipes.lsm.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;
    private static final long SEED = 0;
    private static final int BLOCK_BYTES = 4096;

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("lsm-tree-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.input("seed", Long.toString(SEED));
        h.input("block_bytes", Integer.toString(BLOCK_BYTES));
        h.meta("subms.recipe.slug", "subms-lsm-tree");
        h.meta("subms.recipe.category", "storage");

        Path tmp = Files.createTempDirectory("subms-lsm-features-");
        try {
            benchWal(h, tmp);
            benchTiered(h);
            benchLeveled(h);
            benchSnapshot(h);
            benchLz4(h);
            benchZstd(h);
            benchBlockCache(h);
        } finally {
            deleteRecursive(tmp);
        }

        h.writeJson(System.out);
    }

    private static void benchWal(SubMsPerfHarness h, Path dir) throws IOException {
        h.meta("subms.workload.feature", "wal");
        Path path = dir.resolve("bench.wal");
        Files.deleteIfExists(path);

        byte[] value = "value-payload-bytes".getBytes(java.nio.charset.StandardCharsets.UTF_8);

        // Precompute keys so a warmup pass and the measured pass append the
        // same shaped records; the warmup index wraps back over the array.
        String[] keys = new String[ENTRIES];
        Lcg rng = new Lcg(SEED);
        for (int i = 0; i < ENTRIES; i++) keys[i] = "k" + Integer.toUnsignedString(rng.nextU32());

        // wal_put: append cost per write (buffered + flush, no fsync). Warm
        // logPut on a throwaway log so the JIT is hot without inflating the
        // log that wal_replay scans afterward.
        try (WriteAheadLog warm = new WriteAheadLog(dir.resolve("warm.wal"))) {
            int w = Math.min(ENTRIES, 20_000);
            for (int i = 0; i < w; i++) {
                try {
                    warm.logPut(keys[i % keys.length], value);
                } catch (IOException e) {
                    throw new UncheckedIOException(e);
                }
            }
        }
        Files.deleteIfExists(dir.resolve("warm.wal"));

        try (WriteAheadLog wal = new WriteAheadLog(path)) {
            SubMsPerfHarness.Stage stage = h.stage("wal_put", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            for (int i = 0; i < ENTRIES; i++) {
                String key = keys[i];
                stage.time(() -> {
                    try {
                        wal.logPut(key, value);
                    } catch (IOException e) {
                        throw new UncheckedIOException(e);
                    }
                });
            }
            wal.sync();
        }

        // wal_replay: whole-log scan + CRC verify. One replay reads the entire
        // populated log, so each sample is a full ENTRIES-record recovery;
        // repeat to get a distribution. O(N) per sample - stays above 1 ms
        // even warm; the warmup only steadies the number.
        final int replays = 200;
        SubMsPerfHarness.Stage stage = h.stage("wal_replay", replays).withKind(SubMsStageKind.BATCH_OP);
        stage.warmThenTime(Math.min(replays, 50), replays, () -> {
            try {
                WriteAheadLog.replay(path).size();
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        });

        Files.deleteIfExists(path);
    }

    private static void benchTiered(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "tiered-compaction");
        TieredManifest manifest = new TieredManifest();
        Lcg rng = new Lcg(SEED);
        for (long id = 0; id < 50L; id++) {
            List<TieredRun.Entry> entries = new ArrayList<>(32);
            for (int j = 0; j < 32; j++) {
                entries.add(new TieredRun.Entry("k" + Integer.toUnsignedString(rng.nextU32()), VALUE_V));
            }
            manifest.push(0, new TieredRun(id, entries));
        }
        TieredCompactionPlanner planner = new TieredCompactionPlanner(50);

        // tiered_plan: scan the manifest and decide which level to compact.
        SubMsPerfHarness.Stage stage = h.stage("tiered_plan", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        stage.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, () -> planner.pickLevel(manifest));
    }

    private static void benchLeveled(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "leveled-compaction");
        LeveledManifest manifest = new LeveledManifest();
        Lcg rng = new Lcg(SEED);
        // L0 holds a few overlapping runs; L1+ hold disjoint runs sorted by key.
        for (long id = 0; id < 4L; id++) {
            List<TieredRun.Entry> entries = new ArrayList<>(32);
            for (int j = 0; j < 32; j++) {
                entries.add(new TieredRun.Entry("k" + Integer.toUnsignedString(rng.nextU32()), VALUE_V));
            }
            manifest.push(0, new LeveledRun(id, entries));
        }
        for (long id = 4; id < 50L; id++) {
            long base = (id - 4) * 1000;
            List<TieredRun.Entry> entries = new ArrayList<>(32);
            for (int j = 0; j < 32; j++) {
                entries.add(new TieredRun.Entry(String.format("k%08d", base + j), VALUE_V));
            }
            manifest.push(1, new LeveledRun(id, entries));
        }
        LeveledCompactionPlanner planner = new LeveledCompactionPlanner(64_000, 10, 4);

        // leveled_plan: L0-run-limit check + per-level byte-budget scan.
        SubMsPerfHarness.Stage stage = h.stage("leveled_plan", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        stage.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, () -> planner.pickLevel(manifest));
    }

    private static void benchSnapshot(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "snapshot");
        List<Long> ids = new ArrayList<>(50);
        for (long i = 0; i < 50L; i++) ids.add(i);
        SnapshotManager mgr = new SnapshotManager(new SnapshotManifest(ids));

        // snapshot: id allocation + immutable manifest reference grab.
        SubMsPerfHarness.Stage snap = h.stage("snapshot", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        snap.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> mgr.snapshot());

        // get_on_snapshot: a read resolving against a held snapshot's manifest -
        // walk the captured SSTable id list newest-to-oldest looking for a target.
        Snapshot held = mgr.snapshot();
        List<Long> sstableIds = held.sstableIds();
        Lcg rng = new Lcg(SEED);
        long[] targets = new long[ENTRIES];
        for (int i = 0; i < ENTRIES; i++) targets[i] = Integer.toUnsignedLong(rng.bounded(60));
        SubMsPerfHarness.Stage get = h.stage("get_on_snapshot", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        get.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, (int i) -> {
            long target = targets[i % targets.length];
            boolean found = false;
            for (int idx = sstableIds.size() - 1; idx >= 0; idx--) {
                if (sstableIds.get(idx) == target) {
                    found = true;
                    break;
                }
            }
            consume(found);
        });
    }

    private static void benchLz4(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "lz4");
        Lz4BlockCompressor c = new Lz4BlockCompressor();
        byte[] block = representativeBlock();
        byte[] encoded = c.compress(block);

        SubMsPerfHarness.Stage comp = h.stage("lz4_compress_block", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        comp.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, () -> c.compress(block));

        SubMsPerfHarness.Stage dec = h.stage("lz4_decompress_block", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        dec.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, () -> {
            try {
                c.decompress(encoded);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        });
    }

    private static void benchZstd(SubMsPerfHarness h) throws IOException {
        h.meta("subms.workload.feature", "zstd");
        ZstdBlockCompressor c = new ZstdBlockCompressor();
        byte[] block = representativeBlock();
        byte[] encoded = c.compress(block);

        SubMsPerfHarness.Stage comp = h.stage("zstd_compress_block", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        comp.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, () -> {
            try {
                c.compress(block);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        });

        SubMsPerfHarness.Stage dec = h.stage("zstd_decompress_block", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        dec.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES, () -> {
            try {
                c.decompress(encoded);
            } catch (IOException e) {
                throw new UncheckedIOException(e);
            }
        });
    }

    private static void benchBlockCache(SubMsPerfHarness h) {
        h.meta("subms.workload.feature", "block-cache-integration");
        final int cacheCap = 1024;
        LruBlockCache cache = new LruBlockCache(cacheCap);
        byte[] block = representativeBlock();
        for (long i = 0; i < cacheCap; i++) {
            cache.put(new BlockKey(i % 8, i * BLOCK_BYTES), block);
        }

        // cache_get_cached: warm hit - lock, map lookup, LRU move-to-front.
        // get is read-only on residency (it reorders, never evicts), so the
        // warmup pass keeps all cacheCap keys present for the measured hits.
        Lcg rng = new Lcg(SEED);
        BlockKey[] hitKeys = new BlockKey[ENTRIES];
        for (int n = 0; n < ENTRIES; n++) {
            long i = Integer.toUnsignedLong(rng.bounded(cacheCap));
            hitKeys[n] = new BlockKey(i % 8, i * BLOCK_BYTES);
        }
        SubMsPerfHarness.Stage hit = h.stage("cache_get_cached", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        hit.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES,
            (int n) -> cache.get(hitKeys[n % hitKeys.length]));

        // cache_get_miss: lock, map lookup, miss-counter bump.
        Lcg missRng = new Lcg(SEED ^ 0x9E3779B9L);
        BlockKey[] missKeys = new BlockKey[ENTRIES];
        for (int n = 0; n < ENTRIES; n++) {
            missKeys[n] = new BlockKey(999, Integer.toUnsignedLong(missRng.nextU32()));
        }
        SubMsPerfHarness.Stage miss = h.stage("cache_get_miss", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
        miss.warmThenTime(Math.min(ENTRIES, 20_000), ENTRIES,
            (int n) -> cache.get(missKeys[n % missKeys.length]));
    }

    /** Reused tiny value payload; matches the Rust bench's {@code b"v"}. */
    private static final byte[] VALUE_V = {(byte) 'v'};

    /**
     * A representative ~4KB SSTable data block: repeating record-shaped text so
     * the compressors have realistic-but-not-degenerate redundancy.
     */
    private static byte[] representativeBlock() {
        byte[] pattern = "key-0000042\0present\0value-payload-bytes-for-block|"
                .getBytes(java.nio.charset.StandardCharsets.UTF_8);
        byte[] out = new byte[BLOCK_BYTES];
        for (int i = 0; i < BLOCK_BYTES; i++) out[i] = pattern[i % pattern.length];
        return out;
    }

    /** Keep the JIT from eliding a pure-read result. */
    @SuppressWarnings("unused")
    private static void consume(boolean b) {
        if (b) BLACK_HOLE++;
    }

    private static volatile long BLACK_HOLE;

    private static void deleteRecursive(Path dir) {
        try {
            if (!Files.exists(dir)) return;
            try (var walk = Files.walk(dir)) {
                walk.sorted((a, b) -> b.getNameCount() - a.getNameCount())
                    .forEach(p -> {
                        try {
                            Files.deleteIfExists(p);
                        } catch (IOException ignored) {
                            // best-effort cleanup
                        }
                    });
            }
        } catch (IOException ignored) {
            // best-effort cleanup
        }
    }

    /** Deterministic LCG matching the central {@code subms::SubMsLcg}. */
    private static final class Lcg {
        private long state;

        Lcg(long seed) {
            this.state = seed | 1L;
        }

        int nextU32() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return (int) (state >>> 32);
        }

        int bounded(int n) {
            if (n == 0) return 0;
            return Integer.remainderUnsigned(nextU32(), n);
        }
    }
}
