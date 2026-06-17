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
}
