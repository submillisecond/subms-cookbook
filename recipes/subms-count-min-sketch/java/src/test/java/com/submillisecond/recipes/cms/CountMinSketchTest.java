package com.submillisecond.recipes.cms;

import org.junit.jupiter.api.Test;

import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CountMinSketchTest {

    private static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b));
        return sb.toString();
    }

    @Test
    void widthRoundedUpToPowerOfTwo() {
        assertEquals(1024, new CountMinSketch(5, 1000).width());
    }

    @Test
    void estimateAtOrAboveTrueCount() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        for (int i = 0; i < 1000; i++) cms.add("hot");
        for (int i = 0; i < 10; i++) cms.add("warm");
        assertTrue(cms.estimate("hot") >= 1000);
        assertTrue(cms.estimate("warm") >= 10);
        assertEquals(0, cms.estimate("absent"));
    }

    @Test
    void overEstimationBounded() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        for (int i = 0; i < 1000; i++) cms.add("k" + i);
        cms.add("HOT");
        int est = cms.estimate("HOT");
        assertTrue(est >= 1);
        assertTrue(est < 10, "got " + est);
    }

    @Test
    void depthFloorIsTwo() {
        assertTrue(new CountMinSketch(0, 1024).depth() >= 2);
    }

    @Test
    void depthClampedToMaxDepth() {
        // Rows past MAX_DEPTH would never be read by add or estimate, so the
        // constructor clamps rather than allocating a matrix it will not use.
        CountMinSketch cms = new CountMinSketch(100, 1024);
        assertEquals(CountMinSketch.MAX_DEPTH, cms.depth());
        assertEquals((long) CountMinSketch.MAX_DEPTH * 1024 * 4, cms.heapBytes());
    }

    @Test
    void unseenKeyReturnsZero() {
        assertEquals(0, new CountMinSketch(5, 16384).estimate("never"));
    }

    @Test
    void singleIncrementAtLeastOne() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        cms.add("only");
        assertTrue(cms.estimate("only") >= 1);
    }

    @Test
    void estimatesGrowMonotonically() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        int prev = 0;
        for (int i = 0; i < 1000; i++) {
            cms.add("rising");
            int cur = cms.estimate("rising");
            assertTrue(cur >= prev);
            prev = cur;
        }
    }

    @Test
    void manyDistinctKeysDontInflateUnrelated() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        cms.add("focus");
        for (int i = 0; i < 1000; i++) cms.add("noise-" + i);
        int est = cms.estimate("focus");
        assertTrue(est >= 1);
        assertTrue(est < 100, "over-estimation too high: " + est);
    }

    @Test
    void dAndWAccessors() {
        CountMinSketch cms = new CountMinSketch(7, 8192);
        assertEquals(7, cms.depth());
        assertEquals(8192, cms.width());
        assertEquals(0L, cms.seed());
        assertTrue(cms.toString().contains("depth=7"));
    }

    @Test
    void neverAddedKeyOverEstimateStaysSmall() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        cms.add("present");
        int absent = cms.estimate("absent-and-far-from-present");
        assertTrue(absent <= 1, "absent key over-estimate stays small: " + absent);
    }

    @Test
    void weightedAddMatchesRepeatedAdd() {
        CountMinSketch bulk = new CountMinSketch(5, 4096);
        CountMinSketch oneAtATime = new CountMinSketch(5, 4096);
        bulk.addN("ESZ5", 250);
        for (int i = 0; i < 250; i++) oneAtATime.add("ESZ5");
        assertEquals(oneAtATime.estimate("ESZ5"), bulk.estimate("ESZ5"));
        assertEquals(oneAtATime.total(), bulk.total());
    }

    @Test
    void zeroWeightAddIsANoop() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        cms.addN("k", 0);
        assertEquals(0, cms.estimate("k"));
        assertEquals(0L, cms.total());
        assertTrue(cms.isEmpty());
    }

    @Test
    void negativeWeightIsRejected() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        assertThrows(IllegalArgumentException.class, () -> cms.addN("k", -1));
        assertThrows(IllegalArgumentException.class, () -> cms.addBytesN(new byte[] {1}, -1));
        assertThrows(IllegalArgumentException.class, () -> cms.addU64N(1L, -1));
    }

    @Test
    void totalCountsWeightNotKeys() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        cms.add("a");
        cms.add("b");
        cms.addN("c", 98);
        assertEquals(100L, cms.total());
        assertFalse(cms.isEmpty());
    }

    @Test
    void clearResetsCountersAndVolume() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        for (int i = 0; i < 500; i++) cms.add("k" + i);
        assertTrue(cms.occupancy() > 0.0);
        cms.clear();
        assertEquals(0L, cms.total());
        assertTrue(cms.isEmpty());
        assertEquals(0, cms.estimate("k1"));
        assertEquals(0.0, cms.occupancy());
        assertEquals(5, cms.depth());
        assertEquals(4096, cms.width());
    }

    @Test
    void byteAndStringKeysHashIdentically() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 4096);
        a.add("ESZ5");
        b.addBytes("ESZ5".getBytes(StandardCharsets.UTF_8));
        assertEquals(1, a.estimateBytes("ESZ5".getBytes(StandardCharsets.UTF_8)));
        assertEquals(a.estimate("ESZ5"), b.estimate("ESZ5"));
        b.addBytesN("NQZ5".getBytes(StandardCharsets.UTF_8), 4);
        assertEquals(4, b.estimate("NQZ5"));
    }

    @Test
    void onTheFlyUtf8HashingMatchesGetBytes() {
        // The add path encodes UTF-8 as it hashes rather than allocating a
        // byte[] per call. Any divergence here would break the Java/Rust byte
        // equivalence and the byte[] overloads at once.
        String[] corpus = {
            "", "ESZ5", "a",
            "\u00e9quit\u00e9",              // 2-byte sequences
            "\u00a3100",
            "\u4e2d\u6587",                  // 3-byte sequences
            "\ud83d\ude80rocket",            // surrogate pair, 4 bytes
            "tail\ud83d\ude80",
            "lone-high\ud800",               // unpaired surrogates map to '?'
            "\udc00-lone-low",
            "\ud800\ud800",
            "mix \u00e9 \u4e2d \ud83d\ude80 end"
        };
        for (String s : corpus) {
            CountMinSketch viaString = new CountMinSketch(4, 1024);
            CountMinSketch viaBytes = new CountMinSketch(4, 1024);
            viaString.add(s);
            viaBytes.addBytes(s.getBytes(StandardCharsets.UTF_8));
            assertArrayEquals(viaBytes.toBytes(), viaString.toBytes(), "diverged on a key");
        }
    }

    @Test
    void longKeysMatchTheirLittleEndianBytes() {
        CountMinSketch a = new CountMinSketch(5, 4096);
        CountMinSketch b = new CountMinSketch(5, 4096);
        long key = 987654321L;
        byte[] le = new byte[8];
        for (int i = 0; i < 8; i++) le[i] = (byte) (key >>> (8 * i));
        a.addU64(key);
        b.addBytes(le);
        assertEquals(1, a.estimateU64(key));
        assertEquals(b.estimateBytes(le), a.estimateU64(key));
        a.addU64N(42L, 7);
        assertEquals(7, a.estimateU64(42L));
        // Negative longs are hashed as raw 64 bits, not rejected.
        a.addU64(-1L);
        assertEquals(1, a.estimateU64(-1L));
    }

    @Test
    void seedChangesTheHashFamily() {
        CountMinSketch a = new CountMinSketch(4, 64, 0L);
        CountMinSketch b = new CountMinSketch(4, 64, 0xdeadbeefL);
        for (int i = 0; i < 500; i++) {
            a.add("n" + i);
            b.add("n" + i);
        }
        assertEquals(0xdeadbeefL, b.seed());
        boolean differs = false;
        for (int i = 0; i < 200 && !differs; i++) {
            differs = a.estimate("probe" + i) != b.estimate("probe" + i);
        }
        assertTrue(differs, "a reseeded sketch must not collide identically");
    }

    @Test
    void errorBoundsReportTheSizing() {
        CountMinSketch cms = new CountMinSketch(5, 16384);
        assertEquals(Math.E / 16384.0, cms.relativeError(), 1e-12);
        assertEquals(1.0 - Math.exp(-5.0), cms.confidence(), 1e-12);
        assertTrue(cms.confidence() > 0.99);
        assertEquals((long) 5 * 16384 * 4, cms.heapBytes());
    }

    @Test
    void suggestedSizingMeetsTheRequestedBudget() {
        int w = CountMinSketch.suggestWidth(0.001);
        assertEquals(1, Integer.bitCount(w));
        assertTrue(Math.E / w <= 0.001);

        int d = CountMinSketch.suggestDepth(0.999);
        assertTrue(1.0 - Math.exp(-d) >= 0.999);
        assertTrue(d <= CountMinSketch.MAX_DEPTH);

        CountMinSketch cms = CountMinSketch.withErrorBounds(0.001, 0.999);
        assertTrue(cms.relativeError() <= 0.001);
        assertTrue(cms.confidence() >= 0.999);
    }

    @Test
    void suggestedSizingClampsDegenerateInput() {
        assertEquals(2, CountMinSketch.suggestDepth(0.0));
        assertEquals(2, CountMinSketch.suggestDepth(-1.0));
        assertEquals(2, CountMinSketch.suggestDepth(Double.NaN));
        assertEquals(CountMinSketch.MAX_DEPTH, CountMinSketch.suggestDepth(1.0));
        assertEquals(CountMinSketch.MAX_DEPTH, CountMinSketch.suggestDepth(1.0 - 1e-12));
        assertEquals(1 << 30, CountMinSketch.suggestWidth(0.0));
        assertEquals(1 << 30, CountMinSketch.suggestWidth(Double.NaN));
        assertEquals(1 << 30, CountMinSketch.suggestWidth(1e-12));
        assertEquals(11L, CountMinSketch.withErrorBoundsSeeded(0.01, 0.99, 11L).seed());
    }

    @Test
    void trueCountSitsInsideTheReportedInterval() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        for (int i = 0; i < 20000; i++) cms.addU64(i % 4000);
        int truth = 5;
        for (long k : new long[] {0L, 137L, 3999L}) {
            int est = cms.estimateU64(k);
            assertTrue(est >= truth, "estimate never below truth");
            assertTrue(Math.max(0, est - cms.errorMargin()) <= truth, "lower bound never above truth");
        }
        assertEquals((int) Math.ceil(cms.relativeError() * 20000.0), cms.errorMargin());

        CountMinSketch named = new CountMinSketch(5, 4096);
        for (int i = 0; i < 100; i++) named.add("ESZ5");
        assertTrue(named.estimateLowerBound("ESZ5") <= 100);
        assertTrue(named.estimate("ESZ5") >= 100);
    }

    @Test
    void emptySketchHasNoErrorMargin() {
        CountMinSketch cms = new CountMinSketch(5, 4096);
        assertEquals(0, cms.errorMargin());
        assertEquals(0, cms.estimateLowerBound("k"));
        assertEquals(0.0, cms.occupancy());
    }

    @Test
    void occupancyTracksTouchedCells() {
        CountMinSketch cms = new CountMinSketch(4, 1024);
        cms.add("one");
        assertEquals(4.0 / (4.0 * 1024.0), cms.occupancy(), 1e-9);
    }

    @Test
    void counterSaturatesInsteadOfWrapping() {
        CountMinSketch cms = new CountMinSketch(2, 4);
        cms.addN("hot", Integer.MAX_VALUE);
        cms.addN("hot", 1000);
        assertEquals(Integer.MAX_VALUE, cms.estimate("hot"));
    }

    @Test
    void snapshotRoundTripsStateAndShape() {
        CountMinSketch cms = new CountMinSketch(5, 1024, 99L);
        for (int i = 0; i < 2000; i++) cms.add("k" + i);
        cms.addN("ESZ5", 250);

        byte[] bytes = cms.toBytes();
        assertEquals(32 + 5 * 1024 * 4, bytes.length);
        CountMinSketch restored = CountMinSketch.fromBytes(bytes);

        assertEquals(5, restored.depth());
        assertEquals(1024, restored.width());
        assertEquals(99L, restored.seed());
        assertEquals(cms.total(), restored.total());
        assertEquals(cms.estimate("ESZ5"), restored.estimate("ESZ5"));
        assertEquals(cms.estimate("k7"), restored.estimate("k7"));
        assertArrayEquals(bytes, restored.toBytes());
    }

    @Test
    void snapshotBytesArePinnedCrossLanguageForm() {
        // The Rust port encodes the same sketch to the same bytes. Changing
        // this fixture is a wire-format break, not a test fix.
        CountMinSketch cms = new CountMinSketch(2, 4, 7L);
        cms.add("ESZ5");
        cms.addN("NQZ5", 3);
        String expected =
            "5355424d53434d53"                    // magic
            + "0100"                              // version 1
            + "0200"                              // depth 2
            + "04000000"                          // width 4
            + "0700000000000000"                  // seed 7
            + "0400000000000000"                  // total 4
            + "03000000010000000000000000000000"  // row 0
            + "01000000030000000000000000000000"; // row 1
        assertEquals(expected, toHex(cms.toBytes()));
    }

    @Test
    void snapshotRejectsAForeignBuffer() {
        byte[] junk = new CountMinSketch(2, 4).toBytes();
        junk[0] = 'X';
        assertEquals("not a count-min-sketch snapshot",
            assertThrows(CountMinSketch.SnapshotException.class,
                () -> CountMinSketch.fromBytes(junk)).getMessage());
        assertEquals("truncated snapshot: expected 32 bytes, got 5",
            assertThrows(CountMinSketch.SnapshotException.class,
                () -> CountMinSketch.fromBytes(new byte[5])).getMessage());
    }

    @Test
    void snapshotRejectsAFutureVersion() {
        byte[] bytes = new CountMinSketch(2, 4).toBytes();
        bytes[8] = 9;
        assertEquals("unsupported snapshot version 9",
            assertThrows(CountMinSketch.SnapshotException.class,
                () -> CountMinSketch.fromBytes(bytes)).getMessage());
    }

    @Test
    void snapshotRejectsABadShapeOrATruncatedTail() {
        byte[] noDepth = new CountMinSketch(2, 4).toBytes();
        noDepth[10] = 0;
        noDepth[11] = 0;
        assertEquals("invalid shape: depth=0, width=4",
            assertThrows(CountMinSketch.SnapshotException.class,
                () -> CountMinSketch.fromBytes(noDepth)).getMessage());

        byte[] oddWidth = new CountMinSketch(2, 4).toBytes();
        oddWidth[12] = 5;
        assertEquals("invalid shape: depth=2, width=5",
            assertThrows(CountMinSketch.SnapshotException.class,
                () -> CountMinSketch.fromBytes(oddWidth)).getMessage());

        byte[] full = new CountMinSketch(2, 4).toBytes();
        byte[] clipped = java.util.Arrays.copyOf(full, full.length - 4);
        assertEquals("truncated snapshot: expected 64 bytes, got 60",
            assertThrows(CountMinSketch.SnapshotException.class,
                () -> CountMinSketch.fromBytes(clipped)).getMessage());
    }

    @Test
    void oneSidedErrorHoldsOverASkewedStream() {
        // Zipf-ish: key i appears (1000 / (i+1)) times. The guarantee under
        // test is the only one the structure makes - the estimate is never
        // below truth.
        CountMinSketch cms = new CountMinSketch(5, 8192);
        Map<String, Integer> truth = new HashMap<>();
        for (int i = 0; i < 3000; i++) {
            int hits = 1000 / (i + 1) + 1;
            String key = "sym-" + i;
            for (int j = 0; j < hits; j++) cms.add(key);
            truth.put(key, hits);
        }
        int margin = cms.errorMargin();
        for (Map.Entry<String, Integer> e : truth.entrySet()) {
            int est = cms.estimate(e.getKey());
            assertTrue(est >= e.getValue(), "under-count on " + e.getKey());
            assertTrue(Math.max(0, est - margin) <= e.getValue(), "lower bound above truth");
        }
    }
}
