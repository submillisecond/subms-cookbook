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
            assertTrue(lsm.sstableCount() >= 1,
                    () -> "expected at least one sstable, got " + lsm.sstableCount());
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
}
