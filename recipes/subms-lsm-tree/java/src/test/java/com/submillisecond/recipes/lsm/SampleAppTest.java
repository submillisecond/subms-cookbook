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
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    private static byte[] bytes(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }

    @Test
    void quickstart() throws IOException {
        // quickstart:begin
        Path dir = Files.createTempDirectory("lsm-quickstart");
        try (LsmTree lsm = new LsmTree(dir, 16_000)) {
            lsm.put("AAPL", "150.10");
            assertEquals(Optional.of("150.10"), lsm.get("AAPL")); // stored key: a hit
            assertEquals(Optional.empty(), lsm.get("ZZZZ"));       // absent: bloom-accelerated miss
        }
        // quickstart:end
    }

    @Test
    void orderJournalScenario() throws IOException {
        Path dir = Files.createTempDirectory("lsm-sample-base-");
        try (LsmTree journal = new LsmTree(dir, 256)) {
            journal.put("ORD-0001", "AAPL,100@150.10");
            journal.put("ORD-0002", "MSFT,50@320.55");
            journal.put("ORD-0003", "GOOG,25@140.20");
            journal.flush();

            journal.put("ORD-0001", "AAPL,100@150.42");
            journal.put("ORD-0004", "NVDA,10@900.00");
            journal.delete("ORD-0002");
            journal.flush();

            assertEquals(Optional.of("AAPL,100@150.42"), journal.get("ORD-0001"), "newest write wins");
            assertTrue(journal.get("ORD-0002").isEmpty(), "cancelled order reads absent");
            assertTrue(journal.get("ORD-9999").isEmpty(), "unknown id is a bloom-accelerated miss");

            List<String> liveIds = new ArrayList<>();
            for (Map.Entry<String, String> e : journal.range("ORD-0001", "ORD-0005")) liveIds.add(e.getKey());
            assertEquals(List.of("ORD-0001", "ORD-0003", "ORD-0004"), liveIds, "sorted, tombstone dropped");
        }
    }

    @Test
    void walReplayRecoversAckedWrites() throws IOException {
        Path dir = Files.createTempDirectory("lsm-sample-wal-");
        Path path = dir.resolve("journal.wal");
        try (WriteAheadLog wal = new WriteAheadLog(path)) {
            wal.logPut("ORD-0100", bytes("AAPL,100@150.10"));
            wal.logPut("ORD-0101", bytes("MSFT,50@320.55"));
            wal.logDelete("ORD-0100");
            wal.sync();
        }
        List<WriteAheadLog.WalEntry> recovered = WriteAheadLog.replay(path);
        assertEquals(3, recovered.size(), "every acked write survives the crash");
        assertTrue(recovered.get(2).isDelete(), "the cancel replays as a tombstone");
    }

    @Test
    void tieredMergePromotesFullLevel() {
        TieredManifest manifest = new TieredManifest();
        for (int i = 0; i < 4; i++) {
            manifest.push(0, new TieredRun(i, List.of(
                    new TieredRun.Entry(String.format("ORD-%04d", i), bytes("fill")))));
        }
        TieredCompactionPlanner planner = new TieredCompactionPlanner(4);
        int level = planner.pickLevel(manifest);
        assertTrue(level >= 0);
        planner.merge(manifest, level, 100);
        assertEquals(0, manifest.levelRunCount(0), "L0 drained");
        assertEquals(1, manifest.levelRunCount(1), "merged run promoted to L1");
        assertEquals(-1, planner.pickLevel(manifest), "single run does not re-trigger");
    }

    @Test
    void leveledCompactionKeepsLevelsDisjoint() {
        LeveledManifest manifest = new LeveledManifest();
        manifest.push(0, new LeveledRun(1, List.of(
                new TieredRun.Entry("AAPL", bytes("150.10")),
                new TieredRun.Entry("MSFT", bytes("320.55")))));
        manifest.push(0, new LeveledRun(2, List.of(
                new TieredRun.Entry("GOOG", bytes("140.20")))));
        manifest.push(1, new LeveledRun(3, List.of(
                new TieredRun.Entry("AAPL", bytes("149.00")),
                new TieredRun.Entry("NVDA", bytes("900.00")))));
        LeveledCompactionPlanner planner = new LeveledCompactionPlanner(1_000_000, 10, 2);
        int from = planner.pickLevel(manifest);
        assertTrue(from >= 0);
        planner.compact(manifest, from, 100);
        assertEquals(0, manifest.levelRunCount(0), "L0 drained into L1");
        assertTrue(manifest.levelIsNonOverlapping(1), "L1 key-disjoint after compaction");
    }

    @Test
    void snapshotIsIsolatedFromLaterPublishes() {
        SnapshotManager manager = new SnapshotManager();
        manager.publish(new SnapshotManifest(List.of(1L, 2L, 3L)));
        Snapshot view = manager.snapshot();
        manager.publish(new SnapshotManifest(List.of(1L, 2L, 3L, 4L, 5L)));
        assertEquals(List.of(1L, 2L, 3L), view.sstableIds(), "held view unchanged by later flush");
        assertEquals(List.of(1L, 2L, 3L, 4L, 5L), manager.currentIds(), "live manifest moved on");
    }

    @Test
    void lz4RoundTripsAndShrinks() throws IOException {
        Lz4BlockCompressor codec = new Lz4BlockCompressor();
        byte[] block = bytes("AAPL,100@150.10;".repeat(256));
        byte[] encoded = codec.compress(block);
        assertTrue(encoded.length < block.length, "repetitive block shrinks");
        assertArrayEquals(block, codec.decompress(encoded), "lossless round trip");
    }

    @Test
    void zstdRoundTripsAndShrinks() throws IOException {
        ZstdBlockCompressor codec = new ZstdBlockCompressor();
        byte[] block = bytes("MSFT,50@320.55;".repeat(256));
        byte[] encoded = codec.compress(block);
        assertTrue(encoded.length < block.length, "cold block shrinks");
        assertArrayEquals(block, codec.decompress(encoded), "lossless round trip");
    }

    @Test
    void blockCacheServesHitsAndEvictsLru() {
        LruBlockCache cache = new LruBlockCache(2);
        BlockKey hot = new BlockKey(1, 0);
        assertTrue(cache.get(hot).isEmpty(), "cold miss");
        cache.put(hot, bytes("AAPL block"));
        assertArrayEquals(bytes("AAPL block"), cache.get(hot).orElseThrow(), "warm hit serves the payload");
        cache.put(new BlockKey(2, 0), bytes("MSFT block"));
        cache.put(new BlockKey(3, 0), bytes("GOOG block"));
        assertFalse(cache.get(hot).isPresent(), "coldest evicted at capacity");
        assertEquals(1, cache.hits());
    }
}
