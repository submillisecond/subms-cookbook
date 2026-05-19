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
}
