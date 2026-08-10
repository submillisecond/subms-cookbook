package com.submillisecond.recipes.cuckoo;

import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CuckooFilterTest {

    @Test
    void insertContainsDelete() {
        CuckooFilter cf = new CuckooFilter(1000);
        for (int i = 0; i < 500; i++) assertTrue(cf.insert("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.contains("k" + i));
        for (int i = 0; i < 500; i++) assertTrue(cf.delete("k" + i));
        for (int i = 0; i < 500; i++) assertFalse(cf.contains("k" + i));
        assertEquals(0, cf.size());
    }

    @Test
    void deleteNonexistent() {
        CuckooFilter cf = new CuckooFilter(100);
        assertFalse(cf.delete("never"));
    }

    @Test
    void emptyContains() {
        assertFalse(new CuckooFilter(100).contains("x"));
    }

    @Test
    void falsePositiveRateUnderThreePercent() {
        int n = 10_000;
        CuckooFilter cf = new CuckooFilter(n);
        for (int i = 0; i < n; i++) cf.insert("present" + i);
        int probes = 10_000;
        int fp = 0;
        for (int i = 0; i < probes; i++) if (cf.contains("absent" + i)) fp++;
        assertTrue(((double) fp / probes) < 0.03, "fpr too high: " + fp);
    }

    @Test
    void bucketCountIsPowerOfTwo() {
        int n = new CuckooFilter(1000).bucketCount();
        assertEquals(Integer.highestOneBit(n), n);
    }

    @Test
    void sizeTracksInsertsAndDeletes() {
        CuckooFilter cf = new CuckooFilter(1000);
        assertEquals(0, cf.size());
        cf.insert("a");
        cf.insert("b");
        assertEquals(2, cf.size());
        cf.delete("a");
        assertEquals(1, cf.size());
        cf.delete("absent");
        assertEquals(1, cf.size());
    }

    @Test
    void isEmptyInitially() {
        assertTrue(new CuckooFilter(100).isEmpty());
    }

    @Test
    void duplicateInsertsIncreaseCount() {
        CuckooFilter cf = new CuckooFilter(100);
        cf.insert("dup");
        cf.insert("dup");
        cf.insert("dup");
        assertEquals(3, cf.size());
        assertTrue(cf.contains("dup"));
        cf.delete("dup");
        cf.delete("dup");
        cf.delete("dup");
        assertFalse(cf.contains("dup"));
        assertEquals(0, cf.size());
    }

    @Test
    void stressInsertContainsDeleteCycle() {
        CuckooFilter cf = new CuckooFilter(2000);
        for (int cycle = 0; cycle < 3; cycle++) {
            for (int i = 0; i < 1000; i++) cf.insert("cycle" + cycle + "-k" + i);
            for (int i = 0; i < 1000; i++) assertTrue(cf.contains("cycle" + cycle + "-k" + i));
            for (int i = 0; i < 1000; i++) cf.delete("cycle" + cycle + "-k" + i);
        }
        assertEquals(0, cf.size());
    }

    @Test
    void deletingMissingKeyIsNoOp() {
        CuckooFilter cf = new CuckooFilter(100);
        cf.insert("a");
        assertEquals(1, cf.size());
        cf.delete("never-added");
        assertEquals(1, cf.size(), "delete of missing must not decrement size");
        assertTrue(cf.contains("a"), "delete of missing must not remove the real entry");
    }

    private static List<String> fillToSaturation(CuckooFilter cf, boolean stopOnFirstRefusal) {
        List<String> accepted = new ArrayList<>();
        for (int i = 0; i < 4096; i++) {
            String key = "k" + i;
            if (cf.insert(key)) {
                accepted.add(key);
            } else if (stopOnFirstRefusal) {
                break;
            }
        }
        return accepted;
    }

    @Test
    void saturationNeverProducesAFalseNegative() {
        // The eviction chain runs out of moves long before 4096 keys fit in 8
        // slots. Every key the filter said yes to must still be found.
        CuckooFilter cf = new CuckooFilter(1);
        List<String> accepted = fillToSaturation(cf, false);
        assertTrue(accepted.size() < 4096, "a 2-bucket filter must refuse somewhere");
        for (String key : accepted) {
            assertTrue(cf.contains(key), key + " was accepted then lost");
        }
        assertEquals(accepted.size(), cf.size());
    }

    @Test
    void insertIfAbsentSuppressesARepeat() {
        CuckooFilter cf = new CuckooFilter(1000);
        assertTrue(cf.insertIfAbsent("SEQ-1"));
        assertFalse(cf.insertIfAbsent("SEQ-1"), "a repeat is not stored twice");
        assertEquals(1, cf.size());
        assertTrue(cf.insertIfAbsent("SEQ-2"));
        assertEquals(2, cf.size());
        assertTrue(cf.delete("SEQ-1"));
        assertTrue(cf.insertIfAbsent("SEQ-1"), "absent again after delete");
    }

    @Test
    void tryInsertReportsNotEnoughSpace() {
        CuckooFilter cf = new CuckooFilter(1);
        CuckooException err = assertThrows(CuckooException.class, () -> {
            for (int i = 0; i < 4096; i++) cf.tryInsert("k" + i);
        });
        assertEquals(CuckooException.Reason.NOT_ENOUGH_SPACE, err.reason());
    }

    @Test
    void victimIsRehomedOnceADeleteFreesASlot() {
        CuckooFilter cf = new CuckooFilter(1);
        List<String> accepted = fillToSaturation(cf, true);
        assertFalse(cf.insert("blocked"), "saturated filter refuses while the victim is parked");
        assertTrue(cf.delete(accepted.get(0)));
        assertTrue(cf.insert("blocked"), "the freed slot re-homes the victim");
        assertTrue(cf.contains("blocked"));
    }

    @Test
    void clearResetsToEmptyAndKeepsGeometry() {
        CuckooFilter cf = new CuckooFilter(1000);
        int buckets = cf.bucketCount();
        for (int i = 0; i < 500; i++) cf.insert("k" + i);
        cf.clear();
        assertTrue(cf.isEmpty());
        assertEquals(buckets, cf.bucketCount());
        assertEquals(0.0, cf.loadFactor());
        assertFalse(cf.contains("k1"));
        assertTrue(cf.insert("after-clear"));
    }

    @Test
    void byteAndStringApisAgree() {
        CuckooFilter cf = new CuckooFilter(100);
        byte[] key = "ORD-7".getBytes(StandardCharsets.UTF_8);
        assertTrue(cf.insert(key));
        assertTrue(cf.contains("ORD-7"));
        assertTrue(cf.contains(key));
        assertTrue(cf.delete(key));
        assertFalse(cf.contains("ORD-7"));

        byte[] raw = {(byte) 0xff, 0x00, (byte) 0xfe};
        assertTrue(cf.insert(raw));
        assertTrue(cf.contains(raw));
    }

    @Test
    void capacityLoadFactorAndSizeTrackOccupancy() {
        CuckooFilter cf = new CuckooFilter(1000);
        assertEquals(cf.bucketCount() * 4, cf.capacity());
        assertEquals((long) cf.bucketCount() * 4, cf.sizeInBytes());
        assertEquals(0.0, cf.loadFactor());
        for (int i = 0; i < 256; i++) cf.insert("k" + i);
        assertEquals(256.0 / cf.capacity(), cf.loadFactor(), 1e-12);
    }

    @Test
    void estimatedFppRisesWithLoadAndMatchesTheClosedForm() {
        CuckooFilter cf = new CuckooFilter(1000);
        assertEquals(0.0, cf.estimatedFpp());
        for (int i = 0; i < 400; i++) cf.insert("k" + i);
        double low = cf.estimatedFpp();
        for (int i = 400; i < 900; i++) cf.insert("k" + i);
        double high = cf.estimatedFpp();
        assertTrue(high > low, "fpp should rise with occupancy");

        double expected = 1.0 - Math.pow(1.0 - 1.0 / 256.0, 2.0 * 4 * cf.loadFactor());
        assertEquals(expected, high, 1e-12);
        assertTrue(high < 0.04, "fpp " + high + " above the 8-bit ceiling");
    }

    @Test
    void unionMergesASecondFilter() {
        CuckooFilter a = new CuckooFilter(1000);
        CuckooFilter b = new CuckooFilter(1000);
        for (int i = 0; i < 200; i++) {
            a.insert("a" + i);
            b.insert("b" + i);
        }
        a.union(b);
        for (int i = 0; i < 200; i++) {
            assertTrue(a.contains("a" + i));
            assertTrue(a.contains("b" + i), "b" + i + " lost in the merge");
        }
        assertEquals(400, a.size());
    }

    @Test
    void unionRefusesADifferentGeometry() {
        CuckooFilter a = new CuckooFilter(1000);
        CuckooFilter b = new CuckooFilter(100_000);
        CuckooException err = assertThrows(CuckooException.class, () -> a.union(b));
        assertEquals(CuckooException.Reason.GEOMETRY_MISMATCH, err.reason());
        assertTrue(err.getMessage().contains("incompatible cuckoo geometry"));
    }

    @Test
    void unionRefusesWhenTheTargetIsFull() {
        CuckooFilter a = new CuckooFilter(1);
        CuckooFilter b = new CuckooFilter(1);
        for (int i = 0; i < 64; i++) {
            a.insert("a" + i);
            b.insert("b" + i);
        }
        CuckooException err = assertThrows(CuckooException.class, () -> a.union(b));
        assertEquals(CuckooException.Reason.NOT_ENOUGH_SPACE, err.reason());
    }

    private static byte[] serialise(CuckooFilter cf) throws IOException {
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (DataOutputStream out = new DataOutputStream(bytes)) {
            cf.writeTo(out);
        }
        return bytes.toByteArray();
    }

    @Test
    void serialiseRoundTripPreservesMembership() throws IOException {
        CuckooFilter cf = new CuckooFilter(1000);
        for (int i = 0; i < 500; i++) cf.insert("k" + i);
        byte[] buf = serialise(cf);
        assertEquals(17 + cf.bucketCount() * 4, buf.length);

        CuckooFilter reloaded = CuckooFilter.parse(buf, 0, buf.length);
        assertEquals(cf.size(), reloaded.size());
        assertEquals(cf.bucketCount(), reloaded.bucketCount());
        for (int i = 0; i < 500; i++) assertTrue(reloaded.contains("k" + i));
    }

    @Test
    void serialiseRoundTripCarriesTheVictim() throws IOException {
        CuckooFilter cf = new CuckooFilter(1);
        List<String> accepted = fillToSaturation(cf, true);
        byte[] buf = serialise(cf);
        CuckooFilter reloaded = CuckooFilter.parse(buf, 0, buf.length);
        for (String key : accepted) {
            assertTrue(reloaded.contains(key), key + " lost across serialisation");
        }
    }

    @Test
    void parseRejectsMalformedInput() throws IOException {
        assertThrows(IOException.class, () -> CuckooFilter.parse(new byte[4], 0, 4));

        CuckooFilter cf = new CuckooFilter(100);
        cf.insert("k");
        byte[] buf = serialise(cf);

        assertThrows(IOException.class, () -> CuckooFilter.parse(buf, 0, buf.length - 1));

        byte[] badGeometry = buf.clone();
        badGeometry[3] = 3; // bucket count 3 is not a power of two
        assertThrows(IOException.class, () -> CuckooFilter.parse(badGeometry, 0, badGeometry.length));

        byte[] badVictim = buf.clone();
        for (int i = 13; i < 17; i++) badVictim[i] = (byte) 0x7f;
        assertThrows(IOException.class, () -> CuckooFilter.parse(badVictim, 0, badVictim.length));
    }

    /**
     * Pins the exact serialised bytes. The Rust port's {@code wire_format_fixture}
     * test asserts the same string, so a change to either encoder breaks both
     * suites rather than silently forking the format.
     */
    @Test
    void wireFormatFixture() throws IOException {
        CuckooFilter cf = new CuckooFilter(4);
        for (String sym : new String[] {"AAPL", "MSFT", "GOOG"}) assertTrue(cf.insert(sym));
        StringBuilder hex = new StringBuilder();
        for (byte b : serialise(cf)) hex.append(String.format("%02x", b));
        assertEquals("000000020000000000000003000000000098000000a81a0000", hex.toString());
    }
}
