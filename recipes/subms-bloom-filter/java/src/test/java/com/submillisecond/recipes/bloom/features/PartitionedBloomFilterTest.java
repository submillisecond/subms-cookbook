package com.submillisecond.recipes.bloom.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class PartitionedBloomFilterTest {

    @Test
    void addedKeysAlwaysMatch() {
        PartitionedBloomFilter pb = new PartitionedBloomFilter(1000);
        for (int i = 0; i < 1000; i++) pb.add("k" + i);
        for (int i = 0; i < 1000; i++) {
            assertTrue(pb.mightContain("k" + i));
        }
    }

    @Test
    void emptyFilterRejectsEverything() {
        PartitionedBloomFilter pb = new PartitionedBloomFilter(100);
        assertFalse(pb.mightContain("any"));
    }

    @Test
    void fprTargetHoldsWithDefaultSizing() {
        PartitionedBloomFilter pb = new PartitionedBloomFilter(10_000);
        for (int i = 0; i < 10_000; i++) pb.add("present" + i);
        int probes = 100_000;
        int fp = 0;
        for (int i = 0; i < probes; i++) {
            if (pb.mightContain("absent" + i)) fp++;
        }
        double fpr = (double) fp / probes;
        assertTrue(fpr < 0.05, "FPR " + fpr + " exceeded 5%");
    }

    @Test
    void sliceCountEqualsK() {
        PartitionedBloomFilter pb = new PartitionedBloomFilter(100);
        // Indirect: k() is 7 by default, and bitCount = sliceBits*k.
        assertEquals(7, pb.k());
    }

    @Test
    void addToSliceIndependentOfFullAdd() {
        PartitionedBloomFilter pb = new PartitionedBloomFilter(100);
        pb.addToSlice("partial", 0);
        assertFalse(pb.mightContain("partial"), "one-slice add must not produce a hit");

        for (int i = 1; i < pb.k(); i++) pb.addToSlice("partial", i);
        assertTrue(pb.mightContain("partial"), "all-slice manual add becomes a hit");
    }

    @Test
    void addToSliceRejectsBadIndex() {
        PartitionedBloomFilter pb = new PartitionedBloomFilter(100);
        assertThrows(IndexOutOfBoundsException.class, () -> pb.addToSlice("k", 999));
    }
}
