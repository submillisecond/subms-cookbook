package com.submillisecond.recipes.bloom;

import org.junit.jupiter.api.DisplayName;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/**
 * Correctness + statistical-sanity tests for {@link BloomFilter}.
 *
 * The false-positive-rate test sits in this class because the property
 * is part of the BloomFilter's contract (sized for ~1% FPR at 10 bits
 * per key with k=7). A separate "stats" test class would just split a
 * single concern across two files.
 */
final class BloomFilterTest {

    @Test
    @DisplayName("every added key is reported as a possible match")
    void presentKeysAlwaysMatch() {
        BloomFilter bf = new BloomFilter(1000);
        for (int i = 0; i < 1000; i++) bf.add("key" + i);
        for (int i = 0; i < 1000; i++) {
            assertTrue(bf.mightContain("key" + i),
                    () -> "false negative would violate the bloom-filter contract");
        }
    }

    @Test
    @DisplayName("an empty filter rejects every probe")
    void emptyFilterRejects() {
        BloomFilter bf = new BloomFilter(100);
        assertFalse(bf.mightContain("anything"),
                "empty filter must say no");
    }

    @Test
    @DisplayName("on-disk round-trip preserves membership and shape")
    void roundTripSerialisation() throws IOException {
        BloomFilter original = new BloomFilter(500);
        for (int i = 0; i < 500; i++) original.add("k" + i);

        byte[] bytes;
        try (ByteArrayOutputStream baos = new ByteArrayOutputStream();
             DataOutputStream out = new DataOutputStream(baos)) {
            original.writeTo(out);
            bytes = baos.toByteArray();
        }

        BloomFilter parsed = BloomFilter.parse(bytes, 0, bytes.length);
        assertEquals(original.bitCount(), parsed.bitCount(), "bit_count must survive round-trip");
        assertEquals(original.k(),        parsed.k(),        "k must survive round-trip");
        for (int i = 0; i < 500; i++) {
            assertTrue(parsed.mightContain("k" + i),
                    () -> "membership must survive round-trip");
        }
    }

    @Test
    @DisplayName("FPR holds at ~1% with the documented sizing")
    void falsePositiveRateIsRoughly1Percent() {
        // 10 bits/key + k=7 is sized for ~1% FPR; we allow generous statistical
        // headroom (5% threshold) so the test never flakes on cold runs.
        int n = 10_000;
        BloomFilter bf = new BloomFilter(n);
        for (int i = 0; i < n; i++) bf.add("present" + i);

        int probes = 100_000;
        int falsePositives = 0;
        for (int i = 0; i < probes; i++) {
            if (bf.mightContain("absent" + i)) falsePositives++;
        }
        double fpr = (double) falsePositives / probes;
        assertTrue(fpr < 0.05,
                () -> String.format("fpr %.4f exceeds 5%% (expected ~0.01 with 10 bits/key + k=7)", fpr));
    }

    @Test
    @DisplayName("sizing produces k=7 at the documented 10 bits/key")
    void sizingPicksDocumentedK() {
        BloomFilter bf = new BloomFilter(1_000);
        assertEquals(7, bf.k(), "k should be 7 at 10 bits/key");
        assertTrue(bf.bitCount() >= 10_000, "bit_count >= 10n: " + bf.bitCount());
    }

    @Test
    @DisplayName("expectedEntries == 0 yields a defensible minimum filter")
    void zeroEntriesProducesMinimumFilter() {
        BloomFilter bf = new BloomFilter(0);
        assertTrue(bf.bitCount() >= 1, "bit_count >= 1 even at n=0: " + bf.bitCount());
        assertTrue(bf.k() >= 1, "k >= 1 even at n=0: " + bf.k());
        assertFalse(bf.mightContain("never-added"));
    }

    @Test
    @DisplayName("a saturated filter still has no false negatives")
    void smallFilterSaturationKeepsAddedKeys() {
        BloomFilter bf = new BloomFilter(10);
        String[] keys = {"alpha", "beta", "gamma", "delta", "epsilon",
                         "zeta", "eta", "theta", "iota", "kappa",
                         "extra1", "extra2", "extra3"};
        for (String k : keys) bf.add(k);
        for (String k : keys) {
            assertTrue(bf.mightContain(k),
                    () -> "no false negatives even under saturation: " + k);
        }
    }

    @Test
    @DisplayName("parse rejects truncated input")
    void parseRejectsTruncatedInput() {
        org.junit.jupiter.api.Assertions.assertThrows(IOException.class,
                () -> BloomFilter.parse(new byte[]{1, 2, 3}, 0, 3));
    }

    @Test
    @DisplayName("adding the same key twice is idempotent")
    void duplicateAddIsIdempotent() throws IOException {
        BloomFilter bf = new BloomFilter(100);
        bf.add("key");
        byte[] afterOne;
        try (ByteArrayOutputStream baos = new ByteArrayOutputStream();
             DataOutputStream dos = new DataOutputStream(baos)) {
            bf.writeTo(dos);
            afterOne = baos.toByteArray();
        }
        bf.add("key");
        byte[] afterTwo;
        try (ByteArrayOutputStream baos = new ByteArrayOutputStream();
             DataOutputStream dos = new DataOutputStream(baos)) {
            bf.writeTo(dos);
            afterTwo = baos.toByteArray();
        }
        org.junit.jupiter.api.Assertions.assertArrayEquals(afterOne, afterTwo,
                "adding the same key twice must not change the bit pattern");
    }

    @Test
    @DisplayName("empty string is a valid key")
    void emptyStringIsAValidKey() {
        BloomFilter bf = new BloomFilter(100);
        bf.add("");
        assertTrue(bf.mightContain(""));
    }

    @Test
    @DisplayName("long unicode keys are accepted")
    void longUnicodeKeyAccepted() {
        BloomFilter bf = new BloomFilter(100);
        String unicode = "hello-rocket-very-long-string-with-mixed-content-123456789";
        bf.add(unicode);
        assertTrue(bf.mightContain(unicode));
    }

    /**
     * The bytes a 64-bit, k=7 filter holding {alice, bob, carol} serialises to.
     * The Rust suite pins the identical string. This is what makes the wire
     * format a cross-language contract rather than a claim: it caught this port
     * probing Math.floorMod where Rust probes an unsigned remainder, which
     * silently disagreed on ~46% of probe positions.
     */
    private static final String CROSS_LANG_FIXTURE =
            "000000400000000700000001210c6708c21200c4";

    private static byte[] serialise(BloomFilter bf) throws IOException {
        try (ByteArrayOutputStream baos = new ByteArrayOutputStream();
             DataOutputStream out = new DataOutputStream(baos)) {
            bf.writeTo(out);
            return baos.toByteArray();
        }
    }

    private static String toHex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b));
        return sb.toString();
    }

    @Test
    @DisplayName("wire format matches the cross-language fixture")
    void wireFormatMatchesCrossLanguageFixture() throws IOException {
        BloomFilter bf = new BloomFilter(4);
        for (String key : new String[]{"alice", "bob", "carol"}) bf.add(key);
        assertEquals(CROSS_LANG_FIXTURE, toHex(serialise(bf)));
    }

    @Test
    @DisplayName("the cross-language fixture parses back to its members")
    void crossLanguageFixtureParsesBackToItsMembers() throws IOException {
        byte[] bytes = new byte[CROSS_LANG_FIXTURE.length() / 2];
        for (int i = 0; i < bytes.length; i++) {
            bytes[i] = (byte) Integer.parseInt(
                    CROSS_LANG_FIXTURE.substring(i * 2, i * 2 + 2), 16);
        }
        BloomFilter bf = BloomFilter.parse(bytes, 0, bytes.length);
        assertEquals(64, bf.bitCount());
        assertEquals(7, bf.k());
        for (String key : new String[]{"alice", "bob", "carol"}) {
            assertTrue(bf.mightContain(key), () -> key + " survives the fixture");
        }
    }

    @Test
    @DisplayName("clear empties the filter and keeps its geometry")
    void clearEmptiesTheFilterAndKeepsGeometry() {
        BloomFilter bf = new BloomFilter(1000);
        for (int i = 0; i < 1000; i++) bf.add("key" + i);
        int m = bf.bitCount();
        int k = bf.k();
        bf.clear();
        assertEquals(0L, bf.setBits());
        assertEquals(m, bf.bitCount());
        assertEquals(k, bf.k());
        assertFalse(bf.mightContain("key0"));
    }

    @Test
    @DisplayName("union equals a filter built from both key sets")
    void unionEqualsAFilterBuiltFromBothKeySets() throws IOException {
        BloomFilter left = new BloomFilter(1000);
        BloomFilter right = new BloomFilter(1000);
        BloomFilter both = new BloomFilter(1000);
        for (int i = 0; i < 500; i++) {
            left.add("l" + i);
            both.add("l" + i);
            right.add("r" + i);
            both.add("r" + i);
        }
        left.union(right);
        org.junit.jupiter.api.Assertions.assertArrayEquals(
                serialise(both), serialise(left),
                "merged bits must equal the single-array build");
        for (int i = 0; i < 500; i++) {
            assertTrue(left.mightContain("l" + i));
            assertTrue(left.mightContain("r" + i));
        }
    }

    @Test
    @DisplayName("union refuses mismatched geometry")
    void unionRefusesMismatchedGeometry() {
        BloomFilter small = new BloomFilter(100);
        BloomFilter big = new BloomFilter(1000);
        assertFalse(small.isCompatible(big));
        IllegalArgumentException e = org.junit.jupiter.api.Assertions.assertThrows(
                IllegalArgumentException.class, () -> small.union(big));
        assertTrue(e.getMessage().contains("incompatible bloom geometry"), e.getMessage());
    }

    @Test
    @DisplayName("an empty filter reports no occupancy")
    void emptyFilterReportsNoOccupancy() {
        BloomFilter bf = new BloomFilter(10_000);
        assertEquals(0L, bf.setBits());
        assertEquals(0L, bf.approximateElementCount());
        assertEquals(0.0, bf.estimatedFpp());
    }

    @Test
    @DisplayName("approximate element count tracks actual cardinality")
    void approximateElementCountTracksActualCardinality() {
        int n = 5_000;
        BloomFilter bf = new BloomFilter(n);
        for (int i = 0; i < n; i++) bf.add("key" + i);
        double est = bf.approximateElementCount();
        double err = Math.abs(est - n) / n;
        assertTrue(err < 0.05, () -> "estimate " + est + " off by " + err + " from " + n);
    }

    @Test
    @DisplayName("estimated fpp rises as the filter saturates")
    void estimatedFppRisesAsTheFilterSaturates() {
        BloomFilter bf = new BloomFilter(1_000);
        for (int i = 0; i < 1_000; i++) bf.add("key" + i);
        double atDesignPoint = bf.estimatedFpp();
        for (int i = 1_000; i < 10_000; i++) bf.add("key" + i);
        assertTrue(bf.estimatedFpp() > atDesignPoint,
                () -> "overfilling must raise the estimate: " + atDesignPoint);
        assertTrue(atDesignPoint < 0.05, "design point stays near 1%: " + atDesignPoint);
    }

    @Test
    @DisplayName("a saturated filter reports an unusable element count")
    void saturatedFilterReportsAnUnusableElementCount() {
        BloomFilter bf = new BloomFilter(0);
        for (int i = 0; i < 10_000; i++) bf.add("key" + i);
        assertEquals(bf.bitCount(), bf.setBits());
        assertEquals(Long.MAX_VALUE, bf.approximateElementCount());
    }

    @Test
    @DisplayName("parse refuses a header claiming more words than the slice holds")
    void parseRefusesOversizedWordCount() {
        byte[] hostile = new byte[]{
            0, 0, 0, 64,          // bitCount
            0, 0, 0, 7,           // k
            0x40, 0, 0, 0,        // words = 2^30 - would reserve 8 GB
        };
        IOException e = org.junit.jupiter.api.Assertions.assertThrows(IOException.class,
                () -> BloomFilter.parse(hostile, 0, hostile.length));
        assertTrue(e.getMessage().contains("truncated"), e.getMessage());
    }
}
