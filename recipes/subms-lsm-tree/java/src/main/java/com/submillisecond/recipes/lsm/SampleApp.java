package com.submillisecond.recipes.lsm;

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
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Sample app: a tour of {@code subms-lsm-tree}, base API first, then each
 * opt-in feature from the {@code features} subpackage. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.lsm.SampleApp}
 *
 * <p>The framing throughout is an embedded order journal: a per-symbol store
 * of fills keyed by order id, the kind of write-heavy append-shaped state an
 * LSM tree is built for.
 *
 * <ul>
 *   <li>base                    - the order journal: put/get, a bloom miss, a cancel-tombstone, a range scan
 *   <li>wal                     - durable append-before-ack; replay recovers an un-flushed memtable
 *   <li>tiered-compaction       - size-tiered merge for a write-heavy ingest tier
 *   <li>leveled-compaction      - leveled merge for a read-latency-SLA serving tier
 *   <li>snapshot                - a point-in-time read view for an end-of-day report
 *   <li>lz4                     - fast block compression for the hot tier
 *   <li>zstd                    - higher-ratio block compression for the cold tier
 *   <li>block-cache-integration - a read-side LRU in front of block IO
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws IOException {
        baseOrderJournal();
        walDurableLog();
        tieredIngestTier();
        leveledServingTier();
        snapshotEndOfDayReport();
        lz4HotTier();
        zstdColdTier();
        blockCacheReadPath();
    }

    private static byte[] bytes(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }

    /**
     * Base API: a journal of order fills. A small flush threshold rolls a few
     * SSTables so the read path walks more than the memtable. Shows a hit, a
     * bloom-accelerated miss on an id that was never written, a cancel that
     * lands as a tombstone, and a sorted range scan over the live book.
     */
    static void baseOrderJournal() throws IOException {
        System.out.println("== base: embedded order journal ==");
        Path dir = Files.createTempDirectory("lsm-sample-base-");
        try (LsmTree journal = new LsmTree(dir, 256)) {
            journal.put("ORD-0001", "AAPL,100@150.10");
            journal.put("ORD-0002", "MSFT,50@320.55");
            journal.put("ORD-0003", "GOOG,25@140.20");
            journal.flush();                             // roll SSTable_0

            journal.put("ORD-0001", "AAPL,100@150.42");  // amended fill shadows the old one
            journal.put("ORD-0004", "NVDA,10@900.00");
            journal.delete("ORD-0002");                  // cancel: a tombstone
            journal.flush();                             // roll SSTable_1

            String filled = journal.get("ORD-0001").orElseThrow();
            System.out.println("  ORD-0001 -> " + filled);
            if (!"AAPL,100@150.42".equals(filled)) throw new AssertionError("newest write wins");

            // A cancelled order reads as absent - the tombstone shadows the older fill.
            if (journal.get("ORD-0002").isPresent()) throw new AssertionError("cancelled order is absent");

            // Never written: the per-SSTable bloom answers "no" in a few hash probes.
            if (journal.get("ORD-9999").isPresent()) throw new AssertionError("unknown id: bloom-accelerated miss");

            List<String> liveIds = new ArrayList<>();
            for (Map.Entry<String, String> e : journal.range("ORD-0001", "ORD-0005")) liveIds.add(e.getKey());
            System.out.println("  live book " + liveIds + " across " + journal.sstableCount() + " sstables");
            if (!liveIds.equals(List.of("ORD-0001", "ORD-0003", "ORD-0004"))) {
                throw new AssertionError("sorted, tombstone dropped");
            }
        }
    }

    /**
     * wal: the base tree loses an un-flushed memtable on a crash. The
     * write-ahead log appends every mutation before the write is acked, so a
     * replay rebuilds the surviving records on restart. A torn or bad-CRC tail
     * is dropped without poisoning the recovered prefix.
     */
    static void walDurableLog() throws IOException {
        System.out.println("\n== wal: durable append-before-ack ==");
        Path dir = Files.createTempDirectory("lsm-sample-wal-");
        Path path = dir.resolve("journal.wal");
        try (WriteAheadLog wal = new WriteAheadLog(path)) {
            wal.logPut("ORD-0100", bytes("AAPL,100@150.10"));
            wal.logPut("ORD-0101", bytes("MSFT,50@320.55"));
            wal.logDelete("ORD-0100");                   // cancel, logged too
            wal.sync();                                  // force durability, then "crash"
        }

        List<WriteAheadLog.WalEntry> recovered = WriteAheadLog.replay(path);
        System.out.println("  replayed " + recovered.size() + " records after crash");
        if (recovered.size() != 3) throw new AssertionError("every acked write survives");
        if (!recovered.get(2).isDelete()) throw new AssertionError("the cancel replays as a tombstone");
    }

    /**
     * tiered-compaction: a write-heavy ingest tier keeps flushing similar-sized
     * runs. Size-tiered compaction merges N runs at a level into one larger run
     * at the next, keeping write amplification low at the cost of read/space amp.
     */
    static void tieredIngestTier() {
        System.out.println("\n== tiered-compaction: write-heavy ingest tier ==");
        TieredManifest manifest = new TieredManifest();
        for (int i = 0; i < 4; i++) {
            List<TieredRun.Entry> entries = List.of(
                    new TieredRun.Entry(String.format("ORD-%04d", i), bytes("fill")));
            manifest.push(0, new TieredRun(i, entries)); // newest last
        }

        TieredCompactionPlanner planner = new TieredCompactionPlanner(4);
        int level = planner.pickLevel(manifest);
        if (level < 0) throw new AssertionError("level 0 is full at 4 runs");
        planner.merge(manifest, level, 100);
        System.out.println("  merged 4 L0 runs -> " + manifest.levelRunCount(1) + " run at L1");
        if (manifest.levelRunCount(0) != 0) throw new AssertionError("L0 drained");
        if (manifest.levelRunCount(1) != 1) throw new AssertionError("one merged run promoted");
        if (planner.pickLevel(manifest) != -1) throw new AssertionError("a single run does not re-trigger");
    }

    /**
     * leveled-compaction: a serving tier whose contract is a stable read p99.
     * Leveled compaction keeps each level beyond L0 key-disjoint, so a point
     * read probes at most one run per level. The price is higher write amp.
     */
    static void leveledServingTier() {
        System.out.println("\n== leveled-compaction: read-latency-SLA serving tier ==");
        LeveledManifest manifest = new LeveledManifest();
        manifest.push(0, new LeveledRun(1, List.of(
                new TieredRun.Entry("AAPL", bytes("150.10")),
                new TieredRun.Entry("MSFT", bytes("320.55")))));
        manifest.push(0, new LeveledRun(2, List.of(
                new TieredRun.Entry("GOOG", bytes("140.20")))));
        manifest.push(1, new LeveledRun(3, List.of(
                new TieredRun.Entry("AAPL", bytes("149.00")),   // stale, will be shadowed
                new TieredRun.Entry("NVDA", bytes("900.00")))));

        LeveledCompactionPlanner planner = new LeveledCompactionPlanner(1_000_000, 10, 2);
        int from = planner.pickLevel(manifest);
        if (from < 0) throw new AssertionError("L0 over its 2-run limit");
        planner.compact(manifest, from, 100);
        System.out.println("  compacted L0 -> L1: " + manifest.levelRunCount(1) + " run(s) at L1");
        if (manifest.levelRunCount(0) != 0) throw new AssertionError("L0 drained into L1");
        if (!manifest.levelIsNonOverlapping(1)) throw new AssertionError("L1 key-disjoint after compaction");
    }

    /**
     * snapshot: an end-of-day report scans a consistent view while the ingest
     * thread keeps flushing. {@code snapshot()} pins the manifest; publishing a
     * new manifest afterwards does not perturb the held view.
     */
    static void snapshotEndOfDayReport() {
        System.out.println("\n== snapshot: point-in-time end-of-day report ==");
        SnapshotManager manager = new SnapshotManager();
        manager.publish(new SnapshotManifest(List.of(1L, 2L, 3L)));

        Snapshot reportView = manager.snapshot();                          // the report starts here
        manager.publish(new SnapshotManifest(List.of(1L, 2L, 3L, 4L, 5L))); // ingest keeps flushing

        System.out.println("  report sees " + reportView.sstableIds() + ", live set is now " + manager.currentIds());
        if (!reportView.sstableIds().equals(List.of(1L, 2L, 3L))) {
            throw new AssertionError("held view is isolated from later flushes");
        }
        if (!manager.currentIds().equals(List.of(1L, 2L, 3L, 4L, 5L))) {
            throw new AssertionError("the live manifest moved on");
        }
    }

    /**
     * lz4: the hot tier reads constantly, so decompression sits on the read hot
     * path. LZ4 is the fast codec - a lower ratio for a cheaper decode.
     */
    static void lz4HotTier() throws IOException {
        System.out.println("\n== lz4: fast compression for the hot tier ==");
        Lz4BlockCompressor codec = new Lz4BlockCompressor();
        byte[] block = bytes("AAPL,100@150.10;".repeat(256));
        byte[] encoded = codec.compress(block);
        System.out.println("  " + block.length + " bytes -> " + encoded.length + " compressed");
        if (encoded.length >= block.length) throw new AssertionError("repetitive block shrinks");
        if (!java.util.Arrays.equals(codec.decompress(encoded), block)) {
            throw new AssertionError("lossless round trip");
        }
    }

    /**
     * zstd: the cold tier is written once and read rarely, so bytes on disk
     * dominate. Zstd trades CPU for a better ratio than LZ4.
     */
    static void zstdColdTier() throws IOException {
        System.out.println("\n== zstd: higher-ratio compression for the cold tier ==");
        ZstdBlockCompressor codec = new ZstdBlockCompressor();
        byte[] block = bytes("MSFT,50@320.55;".repeat(256));
        byte[] encoded = codec.compress(block);
        System.out.println("  " + block.length + " bytes -> " + encoded.length + " compressed (level " + codec.level() + ")");
        if (encoded.length >= block.length) throw new AssertionError("cold block shrinks");
        if (!java.util.Arrays.equals(codec.decompress(encoded), block)) {
            throw new AssertionError("lossless round trip");
        }
    }

    /**
     * block-cache-integration: a read-side LRU keyed on (sstable_id, block_offset).
     * The read path consults it before touching disk, so a hit skips the IO.
     */
    static void blockCacheReadPath() {
        System.out.println("\n== block-cache-integration: read-side block cache ==");
        LruBlockCache cache = new LruBlockCache(2);
        BlockKey hot = new BlockKey(1, 0);

        if (cache.get(hot).isPresent()) throw new AssertionError("cold: a miss");
        cache.put(hot, bytes("AAPL block"));
        byte[] served = cache.get(hot).orElseThrow();
        System.out.println("  " + cache.hits() + " hit / " + cache.misses() + " miss after one warm read");
        if (!java.util.Arrays.equals(served, bytes("AAPL block"))) throw new AssertionError("the cached payload is served");

        // A third distinct block evicts the least-recently-used entry (cap 2).
        cache.put(new BlockKey(2, 0), bytes("MSFT block"));
        cache.put(new BlockKey(3, 0), bytes("GOOG block"));
        if (cache.get(hot).isPresent()) throw new AssertionError("coldest block evicted at capacity");
    }
}
