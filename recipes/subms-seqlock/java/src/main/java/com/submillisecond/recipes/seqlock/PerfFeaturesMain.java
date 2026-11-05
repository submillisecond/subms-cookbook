package com.submillisecond.recipes.seqlock;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Arrays;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.Map;
import java.util.function.IntToLongFunction;

/**
 * Feature classification bench, the Java mirror of {@code rust/examples/perf_features.rs}. Each
 * feature's write+read round trip is swept across three image widths (1 / 8 / 64 words);
 * {@link SubMsFeatureManifest#classify} decides the category from the shape of that sweep and a
 * single-op p99 per stage is merge-written into {@code ../.subms/features/java.json}.
 *
 * <p>Single-threaded on purpose: a reader under a live writer measures the retry rate, which is
 * a property of the workload, not of the feature.
 *
 * <pre>
 *   mvn -o -q dependency:build-classpath -Dmdep.outputFile=target/cp.txt
 *   java -Xms1g -Xmx1g -cp "target/classes;$(cat target/cp.txt)" \
 *       com.submillisecond.recipes.seqlock.PerfFeaturesMain
 * </pre>
 */
public final class PerfFeaturesMain {
    private static final int[] WIDTHS = {1, 8, 64};
    private static final int ITEMS_PER_SAMPLE = 1_024;
    private static final int SAMPLES = 256;
    private static final int ROUNDS = 7;
    private static final int OPS = 50_000;
    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_MAX_SAMPLES = 1_000;
    private static final int SLOTS = 64;

    private static volatile long sink;

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        long[][] baseSweep = sweep("base/write+read", PerfFeaturesMain::base);
        long baseP50 = baseSweep[WIDTHS.length - 1][1];
        System.err.println("base write+read p50 per " + ITEMS_PER_SAMPLE + "-op sample at 64 words: " + baseP50 + "ns");

        array(manifest, baseP50);
        latch(manifest, baseP50);
        stats(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    private static long base(int n) {
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(n), new long[n]);
        SeqLock<long[]>.Writer w = lock.writer();
        SeqLock<long[]>.Reader r = lock.reader();
        long[] img = new long[n];
        return batched(i -> {
            img[0] = i;
            w.set(img);
            return r.read()[0];
        });
    }

    private static void array(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("array/write+read", n -> {
            SeqLockArray<long[]> table = SeqLockArray.create(SeqLockImage.longs(n), Collections.nCopies(SLOTS, new long[n]));
            SeqLockArray<long[]>.Writer w = table.writer();
            SeqLockArray<long[]>.Reader r = table.reader();
            long[] img = new long[n];
            return batched(i -> {
                img[0] = i;
                w.set(i % SLOTS, img);
                return r.read(i % SLOTS)[0];
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        SeqLockArray<long[]> table = SeqLockArray.create(SeqLockImage.longs(64), Collections.nCopies(SLOTS, new long[64]));
        SeqLockArray<long[]>.Writer w = table.writer();
        SeqLockArray<long[]>.Reader r = table.reader();
        long[] img = new long[64];
        long write = single(i -> {
            img[0] = i;
            w.set(i % SLOTS, img);
            return 0;
        });
        long read = single(i -> r.read(i % SLOTS)[0]);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("slot_write", write);
        p99.put("slot_read", read);
        manifest.setFeature("array", dec.category(), p99, dec.reason());
    }

    private static void latch(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("latch/write+read", n -> {
            SeqLatch<long[]> latch = SeqLatch.create(SeqLockImage.longs(n), new long[n]);
            SeqLatch<long[]>.Writer w = latch.writer();
            SeqLatch<long[]>.Reader r = latch.reader();
            long[] img = new long[n];
            return batched(i -> {
                img[0] = i;
                w.set(img);
                return r.read()[0];
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        SeqLatch<long[]> latch = SeqLatch.create(SeqLockImage.longs(64), new long[64]);
        SeqLatch<long[]>.Writer w = latch.writer();
        SeqLatch<long[]>.Reader r = latch.reader();
        long[] img = new long[64];
        long write = single(i -> {
            img[0] = i;
            w.set(img);
            return 0;
        });
        long read = single(i -> r.read()[0]);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("latch_write", write);
        p99.put("latch_read", read);
        manifest.setFeature("latch", dec.category(), p99, dec.reason());
    }

    private static void stats(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("stats/write+read", n -> {
            SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(n), new long[n]);
            SeqLock<long[]>.Writer w = lock.writer();
            InstrumentedReader<long[]> r = new InstrumentedReader<>(lock.reader());
            long[] img = new long[n];
            return batched(i -> {
                img[0] = i;
                w.set(img);
                return r.read()[0];
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(64), new long[64]);
        lock.writer();
        InstrumentedReader<long[]> r = new InstrumentedReader<>(lock.reader());
        long read = single(i -> r.read()[0]);
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("counted_read", read);
        manifest.setFeature("stats", dec.category(), p99, dec.reason());
    }

    private static long[][] sweep(String label, IntToLongFunction at) {
        long[][] runs = new long[WIDTHS.length][ROUNDS];
        for (int r = 0; r < ROUNDS; r++) {
            for (int k = 0; k < WIDTHS.length; k++) {
                runs[k][r] = at.applyAsLong(WIDTHS[k]);
            }
        }
        long[][] rows = new long[WIDTHS.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(":");
        for (int k = 0; k < WIDTHS.length; k++) {
            long m = Arrays.stream(runs[k]).min().orElse(0);
            rows[k][0] = WIDTHS[k];
            rows[k][1] = m;
            sb.append(" (").append(WIDTHS[k]).append(", ").append(m).append(')');
        }
        sb.append(" raw ").append(Arrays.deepToString(runs));
        System.err.println(sb);
        return rows;
    }

    private static long batched(IntToLongFunction op) {
        int i = 0;
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int s = 0; s < WARM_MAX_SAMPLES && System.nanoTime() < deadline; s++) {
            long acc = 0;
            for (int r = 0; r < ITEMS_PER_SAMPLE; r++) {
                acc += op.applyAsLong(i++);
            }
            sink = acc;
        }
        SubMsPerfHarness h = new SubMsPerfHarness("seqlock-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", SAMPLES);
        for (int s = 0; s < SAMPLES; s++) {
            final int start = i;
            st.time(() -> {
                long acc = 0;
                for (int r = 0; r < ITEMS_PER_SAMPLE; r++) {
                    acc += op.applyAsLong(start + r);
                }
                sink = acc;
            });
            i += ITEMS_PER_SAMPLE;
        }
        return stat(h, true);
    }

    private static long single(IntToLongFunction op) {
        for (int i = 0; i < OPS; i++) {
            sink = op.applyAsLong(i);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("seqlock-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        for (int i = 0; i < OPS; i++) {
            final int k = i;
            st.time(() -> sink = op.applyAsLong(k));
        }
        return stat(h, false);
    }

    private static long stat(SubMsPerfHarness h, boolean median) {
        return SubMsBench.summarize(h).stages().stream()
                .filter(s -> s.name().equals("op"))
                .findFirst()
                .map(s -> median ? s.p50Ns() : s.p99Ns())
                .orElse(0L);
    }
}
