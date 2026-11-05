package com.submillisecond.recipes.fixedpoint;

import com.submillisecond.perf.SubMsBench;
import com.submillisecond.perf.SubMsFeatureManifest;
import com.submillisecond.perf.SubMsP99Source;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.recipes.fixedpoint.features.Currency;
import com.submillisecond.recipes.fixedpoint.features.Money;
import com.submillisecond.recipes.fixedpoint.features.Text;

import java.io.IOException;
import java.nio.file.Path;
import java.nio.file.Paths;
import java.util.Arrays;
import java.util.LinkedHashMap;
import java.util.Map;

/** Feature classification bench, the Java mirror of {@code rust/examples/perf_features.rs}. */
public final class PerfFeaturesMain {
    private static final int[] SIZES = {1_024, 65_536, 4_194_304};
    private static final int CANON = SIZES[SIZES.length - 1];
    private static final int ITEMS_PER_SAMPLE = 1_024;
    private static final int SAMPLES = 256;
    private static final int ROUNDS = 7;
    private static final int OPS = 50_000;
    private static final long WARM_NANOS = 300_000_000L;
    private static final int WARM_MAX_SAMPLES = 1_000;
    private static final Rounding MODE = Rounding.HALF_EVEN;

    private static volatile long sink;

    @FunctionalInterface
    private interface Op {
        long at(int i);
    }

    @FunctionalInterface
    private interface SizedMeasure {
        long at(int n);
    }

    private record Operands(long[] px, long[] qty) {}

    /** {@code n} prices on the one-cent grid and {@code n} whole-lot quantities, as raws. */
    private static Operands operands(int n) {
        long s = 0x9E3779B97F4A7C15L;
        long[] px = new long[n];
        long[] qty = new long[n];
        for (int i = 0; i < n; i++) {
            s = s * 6364136223846793005L + 1442695040888963407L;
            px[i] = (9_500 + Long.remainderUnsigned(s >>> 11, 1_001)) * 10_000_000L;
        }
        for (int i = 0; i < n; i++) {
            s = s * 6364136223846793005L + 1442695040888963407L;
            qty[i] = (1 + Long.remainderUnsigned(s >>> 11, 5_000)) * Fixed.SCALE;
        }
        return new Operands(px, qty);
    }

    public static void main(String[] args) throws IOException {
        Path path = Paths.get("..", ".subms", "features", "java.json").toAbsolutePath().normalize();
        SubMsFeatureManifest manifest = SubMsFeatureManifest.load("java", path);
        manifest.setP99Source(SubMsP99Source.fromEnv(), SubMsP99Source.instanceFromEnv());

        long[][] baseSweep = sweep("base/mul", n -> {
            Operands o = operands(n);
            int mask = n - 1;
            return batched(ITEMS_PER_SAMPLE, i ->
                    Fixed.fromRaw(o.px()[i & mask]).mul(Fixed.fromRaw(o.qty()[i & mask]), MODE).raw());
        });
        long baseP50 = baseSweep[SIZES.length - 1][1];
        System.err.println("base mul p50 per " + ITEMS_PER_SAMPLE + "-op sample: " + baseP50 + "ns");

        text(manifest, baseP50);
        money(manifest, baseP50);
        wide(manifest, baseP50);

        manifest.save(path);
        System.out.print(manifest.toJson());
    }

    // ---------- text: parse + write, no allocation ----------
    private static void text(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("text/parse+write", n -> {
            Operands o = operands(n);
            byte[][] t = texts(o.px());
            int[] len = lengths(o.px());
            int mask = n - 1;
            byte[] out = new byte[32];
            return batched(ITEMS_PER_SAMPLE, i -> {
                Fixed v = Text.parseBytes(t[i & mask], 0, len[i & mask]);
                return Text.writeTo(v, out, 0, 2) + v.raw();
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Operands o = operands(CANON);
        byte[][] t = texts(o.px());
        int[] len = lengths(o.px());
        int mask = CANON - 1;
        long parse = single(i -> Text.parseBytes(t[i & mask], 0, len[i & mask]).raw());
        byte[] out = new byte[32];
        long write = single(i -> Text.writeTo(Fixed.fromRaw(o.px()[i & mask]), out, 0, 2));
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("parse", parse);
        p99.put("write", write);
        manifest.setFeature("text", dec.category(), p99, dec.reason());
    }

    private static byte[][] texts(long[] px) {
        byte[][] t = new byte[px.length][16];
        for (int i = 0; i < px.length; i++) {
            Text.writeTo(Fixed.fromRaw(px[i]), t[i], 0, 2);
        }
        return t;
    }

    private static int[] lengths(long[] px) {
        int[] len = new int[px.length];
        byte[] tmp = new byte[16];
        for (int i = 0; i < px.length; i++) {
            len[i] = Text.writeTo(Fixed.fromRaw(px[i]), tmp, 0, 2);
        }
        return len;
    }

    // ---------- money: currency check + same arithmetic ----------
    private static void money(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("money/add", n -> {
            Money[] a = moneys(operands(n).px());
            Money[] b = moneys(operands(n).qty());
            int mask = n - 1;
            return batched(ITEMS_PER_SAMPLE, i -> a[i & mask].add(b[i & mask]).amount().raw());
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Operands o = operands(CANON);
        Money[] a = moneys(o.px());
        Money[] b = moneys(o.qty());
        int mask = CANON - 1;
        long add = single(i -> a[i & mask].add(b[i & mask]).amount().raw());
        Fixed rate = Fixed.fromParts(1, 84_500_000);
        long convert = single(i -> a[i & mask].convert(rate, Currency.EUR, MODE).amount().raw());
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("money_add", add);
        p99.put("convert", convert);
        manifest.setFeature("money", dec.category(), p99, dec.reason());
    }

    private static Money[] moneys(long[] raws) {
        Money[] m = new Money[raws.length];
        for (int i = 0; i < raws.length; i++) {
            m[i] = new Money(Fixed.fromRaw(raws[i]), Currency.USD);
        }
        return m;
    }

    // ---------- wide: exact 128-bit product accumulation ----------
    private static void wide(SubMsFeatureManifest manifest, long baseP50) {
        long[][] sw = sweep("wide/add_product", n -> {
            Operands o = operands(n);
            int mask = n - 1;
            Fixed128[] acc = {Fixed128.ZERO};
            return batched(ITEMS_PER_SAMPLE, i -> {
                acc[0] = acc[0].addProduct(Fixed.fromRaw(o.px()[i & mask]), Fixed.fromRaw(o.qty()[i & mask]));
                return acc[0].lo();
            });
        });
        SubMsFeatureManifest.Decision dec = SubMsFeatureManifest.classify(sw, baseP50, null);

        Operands o = operands(CANON);
        int mask = CANON - 1;
        Fixed128[] acc = {Fixed128.ZERO};
        long addProduct = single(i -> {
            acc[0] = acc[0].addProduct(Fixed.fromRaw(o.px()[i & mask]), Fixed.fromRaw(o.qty()[i & mask]));
            return acc[0].lo();
        });
        Fixed128[] products = new Fixed128[CANON];
        for (int i = 0; i < CANON; i++) {
            products[i] = Fixed128.product(Fixed.fromRaw(o.px()[i]), Fixed.fromRaw(o.qty()[i]));
        }
        long toFixed = single(i -> products[i & mask].toFixed(MODE).raw());
        Map<String, Long> p99 = new LinkedHashMap<>();
        p99.put("add_product", addProduct);
        p99.put("to_fixed", toFixed);
        manifest.setFeature("wide", dec.category(), p99, dec.reason());
    }

    private static long[][] sweep(String label, SizedMeasure at) {
        long[][] runs = new long[SIZES.length][ROUNDS];
        for (int r = 0; r < ROUNDS; r++) {
            for (int k = 0; k < SIZES.length; k++) {
                runs[k][r] = at.at(SIZES[k]);
            }
        }
        long[][] rows = new long[SIZES.length][2];
        StringBuilder sb = new StringBuilder("sweep ").append(label).append(":");
        for (int k = 0; k < SIZES.length; k++) {
            long m = Long.MAX_VALUE;
            for (long v : runs[k]) {
                m = Math.min(m, v);
            }
            rows[k][0] = SIZES[k];
            rows[k][1] = m;
            sb.append(" (").append(SIZES[k]).append(", ").append(m).append(')');
        }
        sb.append(" raw ").append(Arrays.deepToString(runs));
        System.err.println(sb);
        return rows;
    }

    private static long batched(int reps, Op op) {
        int i = 0;
        long deadline = System.nanoTime() + WARM_NANOS;
        for (int s = 0; s < WARM_MAX_SAMPLES && System.nanoTime() < deadline; s++) {
            long acc = 0;
            for (int r = 0; r < reps; r++) {
                acc += op.at(i++);
            }
            sink = acc;
        }
        SubMsPerfHarness h = new SubMsPerfHarness("fixed-point-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", SAMPLES);
        for (int s = 0; s < SAMPLES; s++) {
            final int start = i;
            st.time(() -> {
                long acc = 0;
                for (int r = 0; r < reps; r++) {
                    acc += op.at(start + r);
                }
                sink = acc;
            });
            i += reps;
        }
        return stat(h, true);
    }

    private static long single(Op op) {
        for (int i = 0; i < OPS; i++) {
            sink = op.at(i);
        }
        SubMsPerfHarness h = new SubMsPerfHarness("fixed-point-feature", "java");
        SubMsPerfHarness.Stage st = h.stage("op", OPS);
        for (int i = 0; i < OPS; i++) {
            final int k = i;
            st.time(() -> sink = op.at(k));
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
