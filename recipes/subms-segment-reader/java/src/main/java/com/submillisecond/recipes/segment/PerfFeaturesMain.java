package com.submillisecond.recipes.segment;

import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;
import com.submillisecond.recipes.segment.features.Crc32SegmentReader;
import com.submillisecond.recipes.segment.features.Crc32SegmentWriter;
import com.submillisecond.recipes.segment.features.IndexedSegmentReader;
import com.submillisecond.recipes.segment.features.Lz4BlockWriter;
import com.submillisecond.recipes.segment.features.Lz4SegmentReader;
import com.submillisecond.recipes.segment.features.MmapSegmentReader;
import com.submillisecond.recipes.segment.features.WalCursorReader;
import com.submillisecond.recipes.segment.features.Xxh3SegmentReader;
import com.submillisecond.recipes.segment.features.Xxh3SegmentWriter;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;

/**
 * Per-feature bench, the Java mirror of {@code rust/examples/perf_features.rs}.
 * Emits one stage per feature variant - base_next, mmap_next, crc32_next,
 * xxh3_next, lz4_next, seek, next_after_seek, read_committed - with the
 * SAME stage names as the Rust bench so the cookbook FeaturePicker columns
 * line up across languages. JSON contract goes to stdout.
 *
 * <p>The mmap path reads a real temp file (that is the whole point of the
 * feature); every other reader takes the in-memory segment, matching how
 * the base reader and the recipe adapter are exercised. The temp file
 * lives under a unique subdir of the system temp dir and is removed at the
 * end.
 *
 * <pre>
 *   java -cp target/classes:&lt;subms&gt;:&lt;lz4&gt; com.submillisecond.recipes.segment.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int ENTRIES = 50_000;

    /** Small fixed-shape payload - "record-{i}" is 8-13 bytes, matching the
     *  recipe adapter's workload so the base number lines up with java.json. */
    private static byte[] payload(int i) {
        return ("record-" + i).getBytes(StandardCharsets.UTF_8);
    }

    /** Build a plain length-prefix segment in memory. */
    private static byte[] buildBase() throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream(ENTRIES * 24);
        try (DataOutputStream dos = new DataOutputStream(baos)) {
            SegmentWriter w = new SegmentWriter(dos);
            for (int i = 0; i < ENTRIES; i++) {
                w.write(payload(i));
            }
        }
        return baos.toByteArray();
    }

    public static void main(String[] args) throws IOException {
        SubMsPerfHarness h = new SubMsPerfHarness("segment-reader-features", "java");
        h.input("entries", Integer.toString(ENTRIES));
        h.meta("subms.recipe.slug", "subms-segment-reader");
        h.meta("subms.recipe.category", "storage");

        byte[] base = buildBase();
        h.meta("segment_bytes", Integer.toString(base.length));

        // ---------- base ----------
        {
            h.meta("subms.workload.feature", "base");
            // Segment readers are single-use cursors; warm nextRecord() on a
            // throwaway reader over the same bytes, then measure on a fresh one.
            SegmentReader warm = new SegmentReader(new DataInputStream(new ByteArrayInputStream(base)));
            for (int i = 0; i < ENTRIES; i++) warm.nextRecord();

            SubMsPerfHarness.Stage s = h.stage("base_next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            SegmentReader r = new SegmentReader(new DataInputStream(new ByteArrayInputStream(base)));
            for (int i = 0; i < ENTRIES; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                r.nextRecord();
                s.record(t0.elapsedNs());
            }
        }

        // ---------- mmap (reads a real temp file) ----------
        {
            h.meta("subms.workload.feature", "mmap");
            Path dir = Files.createTempDirectory("subms-segment-features-");
            Path path = dir.resolve("segment.bin");
            Files.write(path, base);

            // Warm nextRecord() on a throwaway reader over the same file, then
            // measure on a fresh one (the cursor is consumed once).
            try (MmapSegmentReader warm = new MmapSegmentReader(path)) {
                for (int i = 0; i < ENTRIES; i++) warm.nextRecord();
            }

            SubMsPerfHarness.Stage s = h.stage("mmap_next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            try (MmapSegmentReader r = new MmapSegmentReader(path)) {
                for (int i = 0; i < ENTRIES; i++) {
                    SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                    r.nextRecord();
                    s.record(t0.elapsedNs());
                }
            } finally {
                Files.deleteIfExists(path);
                Files.deleteIfExists(dir);
            }
        }

        // ---------- crc32 ----------
        {
            h.meta("subms.workload.feature", "crc32");
            ByteArrayOutputStream baos = new ByteArrayOutputStream(ENTRIES * 28);
            try (DataOutputStream dos = new DataOutputStream(baos)) {
                Crc32SegmentWriter w = new Crc32SegmentWriter(dos);
                for (int i = 0; i < ENTRIES; i++) {
                    w.write(payload(i));
                }
            }
            byte[] buf = baos.toByteArray();
            Crc32SegmentReader warm = new Crc32SegmentReader(new DataInputStream(new ByteArrayInputStream(buf)));
            for (int i = 0; i < ENTRIES; i++) warm.nextRecord();

            SubMsPerfHarness.Stage s = h.stage("crc32_next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            Crc32SegmentReader r = new Crc32SegmentReader(new DataInputStream(new ByteArrayInputStream(buf)));
            for (int i = 0; i < ENTRIES; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                r.nextRecord();
                s.record(t0.elapsedNs());
            }
        }

        // ---------- xxh3 ----------
        {
            h.meta("subms.workload.feature", "xxh3");
            ByteArrayOutputStream baos = new ByteArrayOutputStream(ENTRIES * 32);
            try (DataOutputStream dos = new DataOutputStream(baos)) {
                Xxh3SegmentWriter w = new Xxh3SegmentWriter(dos);
                for (int i = 0; i < ENTRIES; i++) {
                    w.write(payload(i));
                }
            }
            byte[] buf = baos.toByteArray();
            Xxh3SegmentReader warm = new Xxh3SegmentReader(new DataInputStream(new ByteArrayInputStream(buf)));
            for (int i = 0; i < ENTRIES; i++) warm.nextRecord();

            SubMsPerfHarness.Stage s = h.stage("xxh3_next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            Xxh3SegmentReader r = new Xxh3SegmentReader(new DataInputStream(new ByteArrayInputStream(buf)));
            for (int i = 0; i < ENTRIES; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                r.nextRecord();
                s.record(t0.elapsedNs());
            }
        }

        // ---------- lz4 (compressed blocks) ----------
        {
            h.meta("subms.workload.feature", "lz4");
            // Compressible payloads so the writer takes the LZ4 path and the
            // reader actually decompresses on every block.
            ByteArrayOutputStream baos = new ByteArrayOutputStream(ENTRIES * 16);
            try (DataOutputStream dos = new DataOutputStream(baos)) {
                Lz4BlockWriter w = new Lz4BlockWriter(dos);
                for (int i = 0; i < ENTRIES; i++) {
                    String body = "record-" + i + "-" + "ab".repeat(24);
                    w.write(body.getBytes(StandardCharsets.UTF_8));
                }
            }
            byte[] buf = baos.toByteArray();
            Lz4SegmentReader warm = new Lz4SegmentReader(new DataInputStream(new ByteArrayInputStream(buf)));
            for (int i = 0; i < ENTRIES; i++) warm.nextRecord();

            SubMsPerfHarness.Stage s = h.stage("lz4_next", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            Lz4SegmentReader r = new Lz4SegmentReader(new DataInputStream(new ByteArrayInputStream(buf)));
            for (int i = 0; i < ENTRIES; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                r.nextRecord();
                s.record(t0.elapsedNs());
            }
        }

        // ---------- seek-index ----------
        {
            h.meta("subms.workload.feature", "seek-index");
            IndexedSegmentReader r = new IndexedSegmentReader(base);
            int total = r.totalBlocks();

            // Pseudo-random seek targets via a cheap LCG so the seek path hits
            // a spread of index entries + forward scans rather than one offset.
            long[] state = { 0x9e3779b97f4a7c15L };
            int[] seekTargets = new int[ENTRIES];
            for (int i = 0; i < ENTRIES; i++) {
                state[0] = state[0] * 6364136223846793005L + 1442695040888963407L;
                seekTargets[i] = (int) ((state[0] >>> 33) % total);
            }

            // seekToBlock is random-access, so the reader is not consumed; warm
            // both paths in place on the same reader before measuring.
            int warm = Math.min(ENTRIES, 20_000);
            {
                for (int i = 0; i < warm; i++) r.seekToBlock(seekTargets[i % seekTargets.length]);
                SubMsPerfHarness.Stage s = h.stage("seek", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
                for (int target : seekTargets) {
                    SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                    r.seekToBlock(target);
                    s.record(t0.elapsedNs());
                }
            }
            {
                for (int i = 0; i < warm; i++) {
                    r.seekToBlock(seekTargets[i % seekTargets.length]);
                    r.nextRecord();
                }
                SubMsPerfHarness.Stage s = h.stage("next_after_seek", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
                for (int target : seekTargets) {
                    r.seekToBlock(target);
                    SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                    r.nextRecord();
                    s.record(t0.elapsedNs());
                }
            }
        }

        // ---------- wal-cursor ----------
        {
            h.meta("subms.workload.feature", "wal-cursor");
            // Fully committed watermark: each readCommitted() reads one block
            // and advances the cursor, so the reader is consumed once. Warm on
            // a throwaway reader over the same bytes, then measure on a fresh one.
            WalCursorReader warm = new WalCursorReader(base, base.length);
            for (int i = 0; i < ENTRIES; i++) warm.readCommitted();

            SubMsPerfHarness.Stage s = h.stage("read_committed", ENTRIES).withKind(SubMsStageKind.HOT_PATH);
            WalCursorReader r = new WalCursorReader(base, base.length);
            for (int i = 0; i < ENTRIES; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                r.readCommitted();
                s.record(t0.elapsedNs());
            }
        }

        h.writeJson(System.out);
    }
}
