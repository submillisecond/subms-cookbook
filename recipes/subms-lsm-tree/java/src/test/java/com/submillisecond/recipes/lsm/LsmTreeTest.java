package com.submillisecond.recipes.lsm;

import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Comparator;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Black-box correctness tests for {@link LsmTree}. Each case drives the
 * tree through its public API and asserts the observable behaviour the
 * post claims (read newest-wins, tombstones shadow older values,
 * threshold-driven flush, reopen-from-disk).
 *
 * The internal classes {@link Memtable} and {@link SSTable} are not
 * tested in isolation - the bloom-filter integration is meaningful
 * only at the LsmTree level (one bloom per SSTable, exercised by a
 * read path that consults both layers).
 */
final class LsmTreeTest {

    @TempDir
    Path tempRoot;

    private Path dir;

    @BeforeEach
    void freshDir() throws IOException {
        // JUnit's @TempDir gives a clean per-test directory; we just resolve
        // a stable subdir name inside it so test files don't end up in $TMP.
        dir = tempRoot.resolve("lsm");
        Files.createDirectories(dir);
    }

    @AfterEach
    void cleanup() {
        // @TempDir handles deletion, but if a test leaked a file handle on
        // Windows we make a best-effort sweep so the next test starts clean.
        if (!Files.exists(dir)) return;
        try (var stream = Files.walk(dir)) {
            stream.sorted(Comparator.reverseOrder()).forEach(p -> {
                try { Files.deleteIfExists(p); } catch (IOException ignored) {}
            });
        } catch (IOException ignored) {
            // best-effort; @TempDir will retry.
        }
    }

    @Test
    @DisplayName("put + get round-trip through memtable and after a flush")
    void roundTrip() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("k1", "v1");
            lsm.put("k2", "v2");
            assertEquals("v1", lsm.get("k1").orElseThrow(),  "memtable get k1");
            assertEquals("v2", lsm.get("k2").orElseThrow(),  "memtable get k2");
            assertTrue(lsm.get("nope").isEmpty(),             "absent key is empty");

            lsm.flush();
            assertEquals("v1", lsm.get("k1").orElseThrow(), "sstable get after flush");
        }
    }

    @Test
    @DisplayName("a tombstone shadows the same key in an older sstable")
    void tombstoneShadowsOlderValue() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("k", "v");
            lsm.flush();
            lsm.delete("k");
            assertTrue(lsm.get("k").isEmpty(), "in-memtable tombstone wins over old sstable");
            lsm.flush();
            assertTrue(lsm.get("k").isEmpty(), "flushed tombstone wins over old sstable");
        }
    }

    @Test
    @DisplayName("the newest sstable wins on duplicate keys")
    void newerSstableShadowsOlder() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("k", "old");
            lsm.flush();
            lsm.put("k", "new");
            lsm.flush();
            assertEquals("new", lsm.get("k").orElseThrow(),
                    "scan walks newest-first so the second flush wins");
        }
    }

    @Test
    @DisplayName("reopen-from-disk recovers prior flushed state")
    void reopenSeesPriorFlushes() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("durable", "yes");
            lsm.flush();
        }
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            assertEquals("yes", lsm.get("durable").orElseThrow(),
                    "reopen must read the on-disk sstable trailer");
        }
    }

    @Test
    @DisplayName("memtable threshold triggers an automatic flush")
    void flushTriggeredByThreshold() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 32)) {                // 32-byte memtable cap
            for (int i = 0; i < 20; i++) lsm.put("key" + i, "v" + i);
            // The 32-byte threshold rotates repeatedly; the default background flush
            // turns those frozen memtables into SSTables off the write path, so drain
            // to a deterministic point before counting.
            lsm.flush();
            assertTrue(lsm.sstableCount() >= 1,
                    () -> "expected at least one sstable, got " + lsm.sstableCount());
        }
    }

    @Test
    @DisplayName("default flush mode is background")
    void defaultFlushModeIsBackground() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            assertEquals(FlushMode.BACKGROUND, lsm.flushMode());
        }
    }

    @Test
    @DisplayName("sync flush mode rolls SSTables inline")
    void syncFlushModeRollsSstablesInline() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 32)) {
            lsm.setFlushMode(FlushMode.SYNC);
            assertEquals(FlushMode.SYNC, lsm.flushMode());
            for (int i = 0; i < 20; i++) lsm.put("key" + i, "v" + i);
            // Sync mode flushes on the writer thread, so a threshold crossing is
            // visible immediately - no drain needed.
            assertTrue(lsm.sstableCount() >= 1,
                    () -> "sync flush rolls SSTables on the write path");
            for (int i = 0; i < 20; i++) {
                assertEquals(java.util.Optional.of("v" + i), lsm.get("key" + i));
            }
        }
    }

    @Test
    @DisplayName("background flush keeps reads consistent across active/immutable/SSTable")
    void backgroundFlushReadsStayConsistentAcrossTiers() throws IOException {
        // Under a tiny threshold the default background flusher keeps rotating: at
        // any instant a key may live in the active memtable, a frozen memtable still
        // queued for the worker, or an on-disk SSTable. A read must find it wherever
        // it is - never a transient miss during the hand-off.
        try (LsmTree lsm = new LsmTree(dir, 64)) {
            for (int i = 0; i < 2_000; i++) {
                lsm.put(String.format("key%05d", i), "v" + i);
                int older = i / 2;
                assertEquals(java.util.Optional.of("v" + older),
                        lsm.get(String.format("key%05d", older)),
                        "key vanished mid-flush at i=" + i);
            }
            lsm.flush();
            assertTrue(lsm.sstableCount() >= 1);
            for (int i = 0; i < 2_000; i++) {
                assertEquals(java.util.Optional.of("v" + i), lsm.get(String.format("key%05d", i)));
            }
        }
    }

    @Test
    @DisplayName("background flush persists on close")
    void backgroundFlushPersistsOnClose() throws IOException {
        // close() must drain the flush pipeline (queued frozen memtables + the
        // active one) so nothing written is lost when the tree goes away.
        try (LsmTree lsm = new LsmTree(dir, 128)) {
            for (int i = 0; i < 500; i++) lsm.put(String.format("key%04d", i), "v" + i);
            // no explicit flush(): rely on close() via try-with-resources
        }
        try (LsmTree lsm = new LsmTree(dir, 128)) {
            for (int i = 0; i < 500; i++) {
                assertEquals(java.util.Optional.of("v" + i), lsm.get(String.format("key%04d", i)),
                        "key not persisted on close at i=" + i);
            }
        }
    }

    @Test
    @DisplayName("bloom filter does not lose present keys")
    void bloomDoesNotLosePresentKeys() throws IOException {
        // Sanity-check for the SSTable bloom-trailer integration: every
        // present key must always pass the bloom and produce the value.
        try (LsmTree lsm = new LsmTree(dir, 1024)) {
            for (int i = 0; i < 1000; i++) lsm.put("k" + i, "v" + i);
            lsm.flush();
            for (int i = 0; i < 1000; i++) {
                final int idx = i;
                assertEquals("v" + idx, lsm.get("k" + idx).orElseThrow(),
                        () -> "present key k" + idx + " must survive bloom");
            }
            assertTrue(lsm.get("absolutely-not-here").isEmpty(),
                    "absent key returns empty after bloom + scan");
        }
    }

    @Test
    void getOnEmptyTreeReturnsEmpty(@org.junit.jupiter.api.io.TempDir java.nio.file.Path dir) throws Exception {
        try (LsmTree lsm = new LsmTree(dir, 4096)) {
            assertTrue(lsm.get("anything").isEmpty(),
                    "freshly-opened LSM must have no keys");
        }
    }

    @Test
    void overwritePreservesLatestValueAfterFlush(@org.junit.jupiter.api.io.TempDir java.nio.file.Path dir) throws Exception {
        try (LsmTree lsm = new LsmTree(dir, 1024)) {
            lsm.put("k", "v1");
            lsm.flush();
            lsm.put("k", "v2");
            lsm.flush();
            assertEquals("v2", lsm.get("k").orElseThrow(),
                    "later put must shadow earlier put across flushes");
        }
    }

    @Test
    void reopenAfterCloseReadsPersistedKeys(@org.junit.jupiter.api.io.TempDir java.nio.file.Path dir) throws Exception {
        try (LsmTree lsm = new LsmTree(dir, 1024)) {
            lsm.put("durable", "value");
            lsm.flush();
        }
        try (LsmTree reopened = new LsmTree(dir, 1024)) {
            assertEquals("value", reopened.get("durable").orElseThrow(),
                    "data must survive close + reopen");
        }
    }

    @Test
    void manyKeysAcrossMultipleFlushBoundaries(@org.junit.jupiter.api.io.TempDir java.nio.file.Path dir) throws Exception {
        try (LsmTree lsm = new LsmTree(dir, 256)) { // tiny threshold forces many SSTables
            for (int i = 0; i < 500; i++) lsm.put("k" + i, "v" + i);
            lsm.flush();
            // Probe a spread of keys to be sure the merge across SSTables works.
            for (int i : new int[]{0, 1, 17, 200, 333, 499}) {
                final int idx = i;
                assertEquals("v" + idx, lsm.get("k" + idx).orElseThrow(),
                        () -> "key k" + idx + " present after multi-SSTable flush");
            }
        }
    }

    // Resource-growth gate, mirroring the Rust `on_disk_bytes_track_total_writes_not_live_data`.
    // Behaviour and latency tests both miss an LSM's defining cost: rewriting the
    // same key set appends rather than overwrites, so on-disk bytes track TOTAL
    // WRITES, not live data, until something compacts - and the base tree has no
    // automatic compaction. Correct for the shape, but it has to be measured and
    // stated rather than discovered.
    @Test
    @DisplayName("on-disk bytes track total writes, not live data (no automatic compaction)")
    void onDiskBytesTrackTotalWritesNotLiveData(@TempDir Path dir) throws Exception {
        final int keys = 200;
        final String value = "x".repeat(200);
        try (LsmTree lsm = new LsmTree(dir, 16_000)) {
            // One pass: this is the live data, and it never changes.
            for (int i = 0; i < keys; i++) lsm.put(key(i), value);
            lsm.flush();
            long afterFirstPass = dirBytes(dir);

            // Nine more passes over the SAME keys.
            for (int pass = 0; pass < 9; pass++) {
                for (int i = 0; i < keys; i++) lsm.put(key(i), value);
            }
            lsm.flush();
            long afterTenPasses = dirBytes(dir);

            assertEquals(value, lsm.get(key(keys - 1)).orElseThrow(),
                    "rewriting must not lose the latest value");

            double ratio = (double) afterTenPasses / afterFirstPass;
            assertTrue(ratio > 4.0,
                    () -> "10x the writes over a fixed key set should cost far more than 1x on disk "
                            + "(no automatic compaction); got " + ratio + "x");
            assertTrue(ratio < 15.0,
                    () -> "growth must stay proportional to writes, not amplify beyond the append: got "
                            + ratio + "x");
        }
    }

    // The counterpart to the growth gate above, mirroring the Rust
    // `compaction_bounds_on_disk_under_overwrite`. With compaction enabled,
    // rewriting the same key set stays bounded: the merge reclaims the
    // superseded versions, so on-disk tracks live data instead of total writes.
    @Test
    @DisplayName("compaction bounds on-disk bytes under overwrite")
    void compactionBoundsOnDiskUnderOverwrite(@TempDir Path dir) throws Exception {
        final int keys = 200;
        final String value = "x".repeat(200);
        try (LsmTree lsm = new LsmTree(dir, 16_000)) {
            lsm.setCompactionTrigger(4);
            assertEquals(4, lsm.compactionTrigger(), "trigger is readable back");

            for (int i = 0; i < keys; i++) lsm.put(key(i), value);
            lsm.flush();
            lsm.compact();
            long afterFirstPass = dirBytes(dir);

            for (int pass = 0; pass < 9; pass++) {
                for (int i = 0; i < keys; i++) lsm.put(key(i), value);
                lsm.flush();
                lsm.compact();
            }
            long afterTenPasses = dirBytes(dir);

            assertEquals(value, lsm.get(key(keys - 1)).orElseThrow(),
                    "compaction must preserve the newest value per key");

            double ratio = (double) afterTenPasses / afterFirstPass;
            assertTrue(ratio < 2.0,
                    () -> "compaction must bound on-disk under overwrite; got " + ratio + "x ("
                            + afterFirstPass + " -> " + afterTenPasses + " bytes)");
            assertEquals(1, lsm.sstableCount(), "full compaction leaves a single run");
        }
    }

    @Test
    @DisplayName("compaction drops deleted keys and keeps their neighbours")
    void compactionDropsDeletedKeys(@TempDir Path dir) throws Exception {
        final String value = "z".repeat(64);
        try (LsmTree lsm = new LsmTree(dir, 16_000)) {
            for (int i = 0; i < 100; i++) lsm.put(String.format("k%03d", i), value);
            lsm.flush();
            lsm.delete("k050");
            lsm.flush();
            lsm.compact();

            assertTrue(lsm.get("k050").isEmpty(), "deleted key must stay absent after compaction");
            assertEquals(value, lsm.get("k049").orElseThrow(),
                    "surviving keys must be intact after compaction");
        }
    }

    @Test
    @DisplayName("compact is a no-op below two runs")
    void compactIsNoOpBelowTwoRuns(@TempDir Path dir) throws Exception {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            assertEquals(0, lsm.compactionTrigger(), "auto-compaction is off by default");
            lsm.put("a", "1");
            lsm.compact();
            assertEquals(1, lsm.sstableCount(), "one run in, one run out");
            assertEquals("1", lsm.get("a").orElseThrow(), "the value survives a no-op compact");
        }
    }

    private static String key(int i) {
        return String.format("key%05d", i);
    }

    @Test
    @DisplayName("range is sorted ascending within the memtable")
    void rangeSortedWithinMemtable() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("c", "3");
            lsm.put("a", "1");
            lsm.put("b", "2");
            List<Map.Entry<String, String>> got = lsm.range(null, null);
            assertEquals(List.of("a", "b", "c"), keys(got), "range is sorted ascending");
            assertEquals("1", got.get(0).getValue());
        }
    }

    @Test
    @DisplayName("range bounds are half-open: lo inclusive, hi exclusive")
    void rangeHalfOpenBounds() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            for (String k : List.of("a", "b", "c", "d", "e")) lsm.put(k, k);
            assertEquals(List.of("b", "c"), keys(lsm.range("b", "d")),
                    "lo inclusive, hi exclusive");
        }
    }

    @Test
    @DisplayName("range with a single bound is unbounded on the other side")
    void rangeLoOnlyAndHiOnly() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            for (String k : List.of("a", "b", "c", "d")) lsm.put(k, k);
            assertEquals(List.of("c", "d"), keys(lsm.range("c", null)), "lo-only");
            assertEquals(List.of("a", "b"), keys(lsm.range(null, "c")), "hi-only");
        }
    }

    @Test
    @DisplayName("range merges the memtable over sstables newest-wins")
    void rangeMergesMemtableOverSstables() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("a", "old");
            lsm.put("b", "keep");
            lsm.flush();
            lsm.put("a", "new");
            lsm.put("c", "fresh");
            List<Map.Entry<String, String>> got = lsm.range(null, null);
            assertEquals(List.of("a", "b", "c"), keys(got));
            assertEquals("new", value(got, "a"), "newest value wins");
            assertEquals("keep", value(got, "b"));
            assertEquals("fresh", value(got, "c"));
        }
    }

    @Test
    @DisplayName("range drops a key tombstoned in the memtable")
    void rangeDropsTombstonedKeys() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("a", "1");
            lsm.put("b", "2");
            lsm.flush();
            lsm.delete("a");
            assertEquals(List.of("b"), keys(lsm.range(null, null)),
                    "memtable tombstone omits the key from the range");
        }
    }

    @Test
    @DisplayName("a newer flushed tombstone hides the older on-disk value in a range")
    void rangeTombstoneShadowsAcrossRuns() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            lsm.put("k", "v");
            lsm.flush();
            lsm.delete("k");
            lsm.flush();
            assertTrue(lsm.range(null, null).isEmpty(),
                    "the newer tombstone run shadows the older value run");
        }
    }

    @Test
    @DisplayName("range over an empty tree is empty, bounded or not")
    void rangeEmptyAndUnbounded() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            assertTrue(lsm.range(null, null).isEmpty(), "empty tree, no bounds");
            assertTrue(lsm.range("a", "z").isEmpty(), "empty tree, with bounds");
        }
    }

    @Test
    @DisplayName("range binary-searches to lo within a large flushed run")
    void rangeSeeksWithinALargeFlushedRun() throws IOException {
        try (LsmTree lsm = new LsmTree(dir, 1 << 20)) {
            for (int i = 0; i < 500; i++) {
                lsm.put(String.format("key%04d", i), "v" + i);
            }
            lsm.flush(); // one sstable, 500 sorted records - exercises the seek

            List<String> mid = keys(lsm.range("key0100", "key0110"));
            List<String> expected = new java.util.ArrayList<>();
            for (int i = 100; i < 110; i++) expected.add(String.format("key%04d", i));
            assertEquals(expected, mid, "narrow mid-run window seeks to lo, stops at hi");

            assertEquals(3, lsm.range("key0000", "key0003").size(), "lo at the very start");
            assertEquals(2, lsm.range("key0498", null).size(), "lo near the end, unbounded");
            assertTrue(lsm.range("key9999", null).isEmpty(), "lo past every key");
        }
    }

    private static List<String> keys(List<Map.Entry<String, String>> rows) {
        return rows.stream().map(Map.Entry::getKey).toList();
    }

    private static String value(List<Map.Entry<String, String>> rows, String key) {
        return rows.stream()
                .filter(e -> e.getKey().equals(key))
                .map(Map.Entry::getValue)
                .findFirst()
                .orElse(null);
    }

    private static long dirBytes(Path dir) throws IOException {
        try (var walk = Files.walk(dir)) {
            return walk.filter(Files::isRegularFile).mapToLong(p -> {
                try {
                    return Files.size(p);
                } catch (IOException e) {
                    return 0L;
                }
            }).sum();
        }
    }
}
