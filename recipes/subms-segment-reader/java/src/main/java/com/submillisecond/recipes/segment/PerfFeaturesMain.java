package com.submillisecond.recipes.segment;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
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
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.LinkedHashMap;
import java.util.Map;

/**
 * Feature classification bench, the Java mirror of
 * {@code rust/examples/perf_features.rs}. Each feature's representative op is
 * swept across three SEGMENT LENGTHS, {@link SubMsFeatureManifest#classify}
 * DECIDES the category from the shape of that sweep, and the decision plus a
 * measured p99-by-stage is merge-written into
 * {@code ../.subms/features/java.json}.
 *
 * <p>RECORD COUNT is the sweep axis; the record itself is a fixed 4 KiB block.
 * The checksum and compression features cost per BYTE, so growing the payload
 * alongside the segment would leave any slope with two explanations. With the
 * block pinned, a per-record read has to stay flat however long the segment
 * gets, and only an op that walks the whole segment can climb.
 *
 * <p>This replaces the previous shape, which ran every variant at ONE segment
 * length and ASSERTED hot-path via {@code SubMsStageKind.HOT_PATH}. An asserted
 * category is an opinion the bench cannot contradict; a sweep measures it.
 *
 * <pre>
 *   mvn -q exec:java -Dexec.mainClass=com.submillisecond.recipes.segment.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {

    /** One canonical block - the 4 KiB SSTable/segment block size LevelDB and
     *  RocksDB default to. Held FIXED across the sweep. */
    private static final int RECORD_BYTES = 4096;
    /** 1.05 / 4.2 / 16.8 MB of segment - a 16x span that starts inside cache
     *  and ends outside it, so a per-record read that secretly depends on
     *  segment length has somewhere to show it. The span was 4x longer until the
     *  mmap numbers turned into a lottery: a 67 MB mapped segment does not keep
     *  its page cache on a box with under a gigabyte free, so the measured drain
     *  paid faults the warm drain had already paid and the p50 moved
     *  300 -> 2200 ns between runs of unchanged code. Sweep span is worth less
     *  than a working set that stays resident. */
    private static final int[] RECORDS = {256, 1024, 4096};
    private static final int CANON_N = RECORDS[RECORDS.length - 1];
    /** Timed ops per measurement, FIXED across the sweep so a slope has one
     *  cause - only the segment the ops run against grows. A drain repeats
     *  until it has this many samples; every sweep length divides it. */
    private static final int OPS = 65_536;
    /** The open measurement runs far fewer reps than everything else: each one
     *  leaves a mapping of the whole canonical segment behind, and JDK 21 has no
     *  public unmap, so a full-size loop is 65k live mappings of a 67 MB file. */
    private static final int MMAP_OPEN_REPS = 1024;
    private static final int MMAP_OPEN_WARM = 256;
    /** Warm is budgeted by TIME and by op count, never by a fixed rep count. A
     *  fixed count leaves the first sweep point running interpreted while every
     *  later point reuses the compiled method, which reads as a curve that FALLS
     *  with size - the opposite of the structural signal, and just as wrong. */
    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_OPS = 200_000;

    private static final int TOKEN = 16;
    private static final int DICT_TOKENS = 64;
    private static final byte[] DICT = buildDict();

    private static byte[] buildDict() {
        byte[] d = new byte[DICT_TOKENS * TOKEN];
        long s = 0x2545f4914f6cdd1dL;
        for (int i = 0; i < d.length; i++) {
            s ^= s << 13;
            s ^= s >>> 7;
            s ^= s << 17;
            d[i] = (byte) (s >>> 24);
        }
        return d;
    }

    /**
     * Fills one block with a pseudo-random sequence of 16-byte tokens drawn from
     * a fixed 64-entry dictionary. LZ4 lands near 2.5:1 and, more to the point,
     * has to walk a couple of hundred match/literal steps per block.
     *
     * <p>The first attempt was a random quarter repeated three times. It hits a
     * similar ratio and is completely wrong: LZ4 encodes it as one literal run
     * plus one 3 KB match, so the decode is two memcpys and the recipe would
     * have published memcpy throughput as its decompression cost. Compression
     * ratio is not evidence that a decoder is doing work.
     */
    private static void fillRecord(int i, byte[] out) {
        long s = i * 0x9e3779b97f4a7c15L | 1L;
        for (int off = 0; off < out.length; off += TOKEN) {
            s ^= s << 13;
            s ^= s >>> 7;
            s ^= s << 17;
            int t = (int) ((s >>> 33) % DICT_TOKENS) * TOKEN;
            System.arraycopy(DICT, t, out, off, Math.min(TOKEN, out.length - off));
        }
    }

    private static byte[] buildBase(int n) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream(n * (RECORD_BYTES + 4));
        byte[] rec = new byte[RECORD_BYTES];
        try (DataOutputStream dos = new DataOutputStream(baos)) {
            SegmentWriter w = new SegmentWriter(dos);
            for (int i = 0; i < n; i++) {
                fillRecord(i, rec);
                w.write(rec);
            }
        }
        return baos.toByteArray();
    }

    private static byte[] buildCrc32(int n) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream(n * (RECORD_BYTES + 8));
        byte[] rec = new byte[RECORD_BYTES];
        try (DataOutputStream dos = new DataOutputStream(baos)) {
            Crc32SegmentWriter w = new Crc32SegmentWriter(dos);
            for (int i = 0; i < n; i++) {
                fillRecord(i, rec);
                w.write(rec);
            }
        }
        return baos.toByteArray();
    }

    private static byte[] buildXxh3(int n) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream(n * (RECORD_BYTES + 12));
        byte[] rec = new byte[RECORD_BYTES];
        try (DataOutputStream dos = new DataOutputStream(baos)) {
            Xxh3SegmentWriter w = new Xxh3SegmentWriter(dos);
            for (int i = 0; i < n; i++) {
                fillRecord(i, rec);
                w.write(rec);
            }
        }
        return baos.toByteArray();
    }

    private static byte[] buildLz4(int n) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream(n * (RECORD_BYTES / 2));
        byte[] rec = new byte[RECORD_BYTES];
        try (DataOutputStream dos = new DataOutputStream(baos)) {
            Lz4BlockWriter w = new Lz4BlockWriter(dos);
            for (int i = 0; i < n; i++) {
                fillRecord(i, rec);
                w.write(rec);
            }
        }
        return baos.toByteArray();
    }

    private static DataInputStream over(byte[] seg) {
        return new DataInputStream(new ByteArrayInputStream(seg));
    }

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        // Stamp the box these numbers came from. The bench runs wherever it is
        // invoked, so an unstamped manifest is indistinguishable from a fleet
        // capture; the renderer will not publish one it cannot attribute.
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        // The baseline: a plain sequential nextRecord() over a base segment.
        // Every feature decorates that read, so that is what each is measured
        // against.
        byte[] canon = buildBase(CANON_N);
        // The FIRST drain of the process is not a measurement, it is the process
        // warming up: classload, C2 across the whole DataInputStream call chain,
        // and a young generation growing to absorb 4 KiB per read. Probed four
        // times in a row it read 1200, 300, 300, 300 ns, so the 300 ms warm
        // inside one drain does not cover it. This reading is the divisor for
        // every feature category, and an inflated one silently demotes real
        // hot-path features to auxiliary - crc32 landed on the wrong side of the
        // 10%% band on a run where the base read 1000 instead of 300.
        long cold = drain(CANON_N, () -> new SegmentReader(over(canon)),
                SegmentReader::nextRecord, true);
        long baseP50 = drain(CANON_N, () -> new SegmentReader(over(canon)),
                SegmentReader::nextRecord, true);
        System.err.println("base next p50: " + baseP50 + "ns (first pass " + cold
                + "ns, discarded) over " + CANON_N + " x " + RECORD_BYTES + "B records");

        mmap(manifest, baseP50, canon);
        crc32(manifest, baseP50);
        xxh3(manifest, baseP50);
        lz4(manifest, baseP50);
        seekIndex(manifest, baseP50, canon);
        walCursor(manifest, baseP50, canon);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- mmap: zero-copy read out of a mapped file ----------
    private static void mmap(SubMsFeatureManifest manifest, long baseP50, byte[] canon)
            throws IOException {
        Path dir = Files.createTempDirectory("subms-segment-features-");

        // One map per sweep point, rewound between passes. Mapping afresh for
        // every pass would charge each measured read a minor fault - roughly one
        // per 4 KiB record here - and publish the page-fault floor as the read
        // cost. The warm drain touches every page once; what is measured after
        // it is the parse, and the first-touch cost is a property of the OS and
        // the storage, not of this code.
        long[][] sweep = sweep("mmap/next", n -> {
            Path p = dir.resolve("sweep-" + n + ".bin");
            Files.write(p, buildBase(n));
            try (MmapSegmentReader r = new MmapSegmentReader(p)) {
                return drain(n, () -> {
                    r.rewind();
                    return r;
                }, MmapSegmentReader::nextRecord, true);
            } finally {
                Files.deleteIfExists(p);
            }
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Path p = dir.resolve("canon.bin");
        Files.write(p, canon);
        Map<String, Long> p99 = new LinkedHashMap<>();
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            p99.put("next", drain(CANON_N, () -> {
                r.rewind();
                return r;
            }, MmapSegmentReader::nextRecord, false));
        }
        // The feature's other claim: open is constant-time because nothing is
        // read, only mapped.
        p99.put("open", keyed(MMAP_OPEN_REPS, MMAP_OPEN_WARM, i -> {
            SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
            MmapSegmentReader m = new MmapSegmentReader(p);
            long ns = t0.elapsedNs();
            m.close();
            return ns;
        }, false));
        manifest.setFeature("mmap", dec.category(), p99, dec.reason());

        Files.deleteIfExists(p);
        Files.deleteIfExists(dir);
    }

    // ---------- crc32: CRC32C trailer verified per block ----------
    private static void crc32(SubMsFeatureManifest manifest, long baseP50) throws IOException {
        long[][] sweep = sweep("crc32/next", n -> {
            byte[] seg = buildCrc32(n);
            return drain(n, () -> new Crc32SegmentReader(over(seg)),
                    Crc32SegmentReader::nextRecord, true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        byte[] seg = buildCrc32(CANON_N);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("next", drain(CANON_N, () -> new Crc32SegmentReader(over(seg)),
                Crc32SegmentReader::nextRecord, false));
        manifest.setFeature("crc32", dec.category(), p99, dec.reason());
    }

    // ---------- xxh3: xxHash3-64 trailer verified per block ----------
    private static void xxh3(SubMsFeatureManifest manifest, long baseP50) throws IOException {
        long[][] sweep = sweep("xxh3/next", n -> {
            byte[] seg = buildXxh3(n);
            return drain(n, () -> new Xxh3SegmentReader(over(seg)),
                    Xxh3SegmentReader::nextRecord, true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        byte[] seg = buildXxh3(CANON_N);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("next", drain(CANON_N, () -> new Xxh3SegmentReader(over(seg)),
                Xxh3SegmentReader::nextRecord, false));
        manifest.setFeature("xxh3", dec.category(), p99, dec.reason());
    }

    // ---------- lz4: compressed blocks, decompressed on read ----------
    private static void lz4(SubMsFeatureManifest manifest, long baseP50) throws IOException {
        // Ratio is a property of fillRecord, so it is identical at every sweep
        // point; printed because a compression bench with an unstated ratio is
        // not reproducible.
        byte[] canonSeg = buildLz4(CANON_N);
        System.err.printf("lz4 compression ratio: %.2fx%n",
                (double) CANON_N * RECORD_BYTES / canonSeg.length);

        long[][] sweep = sweep("lz4/next", n -> {
            byte[] seg = buildLz4(n);
            return drain(n, () -> new Lz4SegmentReader(over(seg)),
                    Lz4SegmentReader::nextRecord, true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("next", drain(CANON_N, () -> new Lz4SegmentReader(over(canonSeg)),
                Lz4SegmentReader::nextRecord, false));
        manifest.setFeature("lz4", dec.category(), p99, dec.reason());
    }

    // ---------- seek-index: sparse index, random-access positioning ----------
    private static void seekIndex(SubMsFeatureManifest manifest, long baseP50, byte[] canon)
            throws IOException {
        // Swept on seekToBlock, which is the op the feature exists for. The
        // index build in the constructor is the other half of the bargain and it
        // IS O(n), but it is a construction cost paid once per reader, not an op
        // a caller repeats - publishing it as a per-op stage would read as a
        // latency claim on something nobody calls in a loop. The seek itself is
        // a binary search over n/64 entries plus a scan bounded by the 64-block
        // stride, so it should stay flat and only widen as the target span
        // outgrows cache.
        long[][] sweep = sweep("seek-index/seek", n -> {
            IndexedSegmentReader r = new IndexedSegmentReader(buildBase(n));
            int[] targets = seekTargets(r.totalBlocks());
            return keyed(OPS, WARM_OPS, i -> {
                int t = targets[i % targets.length];
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                r.seekToBlock(t);
                return t0.elapsedNs();
            }, true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        IndexedSegmentReader r = new IndexedSegmentReader(canon);
        int[] targets = seekTargets(r.totalBlocks());
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("seek", keyed(OPS, WARM_OPS, i -> {
            int t = targets[i % targets.length];
            SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
            r.seekToBlock(t);
            return t0.elapsedNs();
        }, false));
        // The read that follows a seek lands on a cold line every time, which is
        // the cost a random-access reader actually pays; the sequential base
        // number is prefetched and does not describe it.
        p99.put("next_after_seek", keyed(OPS, WARM_OPS, i -> {
            r.seekToBlock(targets[i % targets.length]);
            SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
            r.nextRecord();
            return t0.elapsedNs();
        }, false));
        manifest.setFeature("seek-index", dec.category(), p99, dec.reason());
    }

    // ---------- wal-cursor: durability watermark on the read path ----------
    private static void walCursor(SubMsFeatureManifest manifest, long baseP50, byte[] canon)
            throws IOException {
        // readCommitted returns ONE block, not everything below the watermark,
        // so this is a per-record op and not a scan - there is no bulk drain in
        // the API to sweep instead. The watermark check is a single comparison
        // on top of the base parse, so a flat curve at base cost is the expected
        // result and a rise would mean the watermark is doing more than it
        // claims.
        long[][] sweep = sweep("wal-cursor/read_committed", n -> {
            byte[] seg = buildBase(n);
            return drain(n, () -> new WalCursorReader(seg, seg.length),
                    WalCursorReader::readCommitted, true);
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sweep, baseP50, null);

        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("read_committed", drain(CANON_N, () -> new WalCursorReader(canon, canon.length),
                WalCursorReader::readCommitted, false));
        p99.put("next_record", drain(CANON_N, () -> new WalCursorReader(canon, canon.length),
                WalCursorReader::nextRecord, false));
        manifest.setFeature("wal-cursor", dec.category(), p99, dec.reason());
    }

    // ---------- harness plumbing ----------

    /**
     * Reads every record of a segment through one reader, timing each read.
     * {@code open} hands back a reader positioned at the head OUTSIDE the timed
     * region - for the in-memory readers that is a fresh construction, for mmap
     * it is a rewind of the single live map, which is what keeps first-touch
     * page faults in the warm loop instead of on the measured pass.
     */
    private static <R> long drain(int n, IoSupplier<R> open, IoConsumer<R> next, boolean median)
            throws IOException {
        long deadline = System.nanoTime() + WARM_NANOS;
        int warmed = 0;
        while (warmed < WARM_OPS && System.nanoTime() < deadline) {
            R r = open.get();
            for (int i = 0; i < n; i++) {
                next.accept(r);
            }
            warmed += n;
        }
        SubMsPerfHarness h = new SubMsPerfHarness("segment-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        int done = 0;
        while (done < OPS) {
            R r = open.get();
            for (int i = 0; i < n; i++) {
                SubMsTimer.SubMsTick t0 = SubMsTimer.tick();
                next.accept(r);
                st.record(t0.elapsedNs());
            }
            done += n;
        }
        return stat(h, median);
    }

    /**
     * A random-access op. {@code op} returns the nanoseconds to record, so an op
     * that needs untimed positioning first (seek, then read) can exclude it
     * without a second closure fighting the first for the reader.
     */
    private static long keyed(int ops, int warmOps, Sample op, boolean median) throws IOException {
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int i = 0; i < warmOps && System.nanoTime() < deadline; i++) {
            op.at(i);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("segment-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", ops);
        for (int i = 0; i < ops; i++) {
            st.record(op.at(i));
        }
        return stat(h, median);
    }

    /**
     * Sweeps and PRINTS the curve. A ratio-compressed or non-monotonic curve
     * classifies as flat and the rows are the only place it shows.
     */
    private static long[][] sweep(String label, SizedMeasure at) throws IOException {
        long[][] rows = new long[RECORDS.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(": ");
        for (int i = 0; i < RECORDS.length; i++) {
            rows[i][0] = RECORDS[i];
            rows[i][1] = at.at(RECORDS[i]);
            sb.append('(').append(rows[i][0]).append(", ").append(rows[i][1]).append(") ");
        }
        System.err.println(sb.toString().trim());
        return rows;
    }

    private static int[] seekTargets(int total) {
        long state = 0x9e3779b97f4a7c15L;
        int[] out = new int[OPS];
        for (int i = 0; i < out.length; i++) {
            state = state * 6364136223846793005L + 1442695040888963407L;
            out[i] = (int) ((state >>> 33) % total);
        }
        return out;
    }

    private static long stat(SubMsPerfHarness h, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }

    @FunctionalInterface
    private interface IoSupplier<R> {
        R get() throws IOException;
    }

    @FunctionalInterface
    private interface IoConsumer<R> {
        void accept(R r) throws IOException;
    }

    @FunctionalInterface
    private interface Sample {
        long at(int i) throws IOException;
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n) throws IOException;
    }

    private PerfFeaturesMain() {}
}
