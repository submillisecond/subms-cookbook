package com.submillisecond.recipes.cuckoo.features;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CompressedCuckooFilterTest {

    @Test
    void roundTripBelowSaturation() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(1000);
        for (int i = 0; i < 500; i++) assertTrue(cf.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.contains("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.delete("k" + i));
        assertEquals(0, cf.size());
    }

    @Test
    void emptyFilterRejectsEverything() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(100);
        assertFalse(cf.contains("never-inserted"));
        assertTrue(cf.isEmpty());
        assertEquals(0, cf.size());
    }

    @Test
    void occupiedBytesGrowsWithInserts() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(500);
        int baseline = cf.occupiedBytes();
        for (int i = 0; i < 200; i++) cf.insert("k" + i);
        assertTrue(cf.occupiedBytes() > baseline, "expected occupancy to grow");
    }

    @Test
    void deleteUnknownReturnsFalse() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(100);
        cf.insert("known");
        assertFalse(cf.delete("never-inserted"));
        assertTrue(cf.contains("known"));
    }

    @Test
    void sortedInvariantHoldsThroughInsertsAndDeletes() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(500);
        for (int i = 0; i < 400; i++) cf.insert("k" + i);
        for (int i = 0; i < 200; i++) cf.delete("k" + i);
        for (int i = 400; i < 500; i++) cf.insert("k" + i);
        byte[][] buckets = cf.bucketsForTest();
        for (byte[] bucket : buckets) {
            int c = bucket[0] & 0xff;
            for (int k = 1; k < c; k++) {
                int prev = bucket[1 + k - 1] & 0xff;
                int cur = bucket[1 + k] & 0xff;
                assertTrue(prev <= cur, "bucket out of sorted order");
            }
        }
    }

    @Test
    void falsePositiveRateInThreePercentRange() {
        int n = 5_000;
        CompressedCuckooFilter cf = new CompressedCuckooFilter(n);
        for (int i = 0; i < n; i++) cf.insert("present" + i);
        int probes = 10_000;
        int fp = 0;
        for (int i = 0; i < probes; i++) {
            if (cf.contains("absent" + i)) fp++;
        }
        double fpr = (double) fp / probes;
        assertTrue(fpr < 0.03, "fpr " + fpr + " too high");
    }

    @Test
    void bucketCountIsPowerOfTwo() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(1000);
        int n = cf.bucketCount();
        assertEquals(0, n & (n - 1));
    }

    @Test
    void duplicateInsertsStackInBucket() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(100);
        cf.insert("dup");
        cf.insert("dup");
        cf.insert("dup");
        assertEquals(3, cf.size());
        assertTrue(cf.contains("dup"));
        cf.delete("dup");
        cf.delete("dup");
        cf.delete("dup");
        assertFalse(cf.contains("dup"));
    }

    private static byte[] serialise(CompressedCuckooFilter cf) throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (DataOutputStream out = new DataOutputStream(bytes)) {
            cf.writeTo(out);
        }
        return bytes.toByteArray();
    }

    @Test
    void compactSerialisationRoundTrip() throws IOException {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(10_000);
        for (int i = 0; i < 3_000; i++) cf.insert("k" + i);
        byte[] buf = serialise(cf);
        // The whole point of the feature: the stream is the live bytes, not the
        // base layout's fixed four slots per bucket.
        assertEquals(17 + cf.occupiedBytes(), buf.length);
        assertTrue(buf.length < 17 + cf.bucketCount() * 4);

        CompressedCuckooFilter reloaded = CompressedCuckooFilter.parse(buf, 0, buf.length);
        assertEquals(cf.size(), reloaded.size());
        assertEquals(cf.bucketCount(), reloaded.bucketCount());
        for (int i = 0; i < 3_000; i++) assertTrue(reloaded.contains("k" + i));
    }

    @Test
    void compactParseRejectsMalformedInput() throws IOException {
        assertThrows(IOException.class, () -> CompressedCuckooFilter.parse(new byte[4], 0, 4));

        CompressedCuckooFilter cf = new CompressedCuckooFilter(100);
        cf.insert("k");
        byte[] buf = serialise(cf);

        assertThrows(IOException.class, () -> CompressedCuckooFilter.parse(buf, 0, buf.length - 1));

        byte[] badGeometry = buf.clone();
        badGeometry[3] = 3;
        assertThrows(IOException.class,
            () -> CompressedCuckooFilter.parse(badGeometry, 0, badGeometry.length));

        byte[] badVictim = buf.clone();
        for (int i = 13; i < 17; i++) badVictim[i] = (byte) 0x7f;
        assertThrows(IOException.class,
            () -> CompressedCuckooFilter.parse(badVictim, 0, badVictim.length));

        byte[] badRun = buf.clone();
        badRun[17] = 99; // a count byte beyond BUCKET_SIZE
        assertThrows(IOException.class,
            () -> CompressedCuckooFilter.parse(badRun, 0, badRun.length));
    }

    /**
     * Pins the compact bytes. The Rust port's {@code compact_wire_format_fixture}
     * asserts the same string.
     */
    @Test
    void compactWireFormatFixture() throws IOException {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(4);
        for (String sym : new String[] {"AAPL", "MSFT", "GOOG"}) assertTrue(cf.insert(sym));
        StringBuilder hex = new StringBuilder();
        for (byte b : serialise(cf)) hex.append(String.format("%02x", b));
        assertEquals("00000002000000000000000300000000000198021aa8", hex.toString());
    }

    @Test
    void saturationNeverProducesAFalseNegative() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(1);
        List<String> accepted = new ArrayList<>();
        for (int i = 0; i < 4096; i++) {
            String key = "k" + i;
            if (cf.insert(key)) accepted.add(key);
        }
        assertTrue(accepted.size() < 4096, "a 2-bucket filter must refuse");
        for (String key : accepted) {
            assertTrue(cf.contains(key), key + " was accepted then lost");
        }
    }

    @Test
    void victimIsRehomedOnceADeleteFreesASlot() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(1);
        List<String> accepted = new ArrayList<>();
        for (int i = 0; i < 4096; i++) {
            String key = "k" + i;
            if (!cf.insert(key)) break;
            accepted.add(key);
        }
        assertFalse(cf.insert("blocked"));
        assertTrue(cf.delete(accepted.get(0)));
        assertTrue(cf.insert("blocked"));
        assertTrue(cf.contains("blocked"));
    }

    @Test
    void clearResetsToEmptyAndKeepsGeometry() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(1000);
        int buckets = cf.bucketCount();
        for (int i = 0; i < 300; i++) cf.insert("k" + i);
        cf.clear();
        assertTrue(cf.isEmpty());
        assertEquals(buckets, cf.bucketCount());
        assertEquals(buckets, cf.occupiedBytes());
        assertFalse(cf.contains("k1"));
    }
}
