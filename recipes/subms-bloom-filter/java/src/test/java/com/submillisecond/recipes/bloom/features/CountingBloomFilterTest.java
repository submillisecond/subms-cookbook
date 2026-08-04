package com.submillisecond.recipes.bloom.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CountingBloomFilterTest {

    @Test
    void addThenRemoveClearsMembership() {
        CountingBloomFilter cb = new CountingBloomFilter(100);
        cb.add("key");
        assertTrue(cb.mightContain("key"));
        cb.remove("key");
        assertFalse(cb.mightContain("key"), "removed key must not match");
    }

    @Test
    void removeOfUnknownIsNoop() {
        CountingBloomFilter cb = new CountingBloomFilter(100);
        cb.add("known");
        cb.remove("unknown-key");
        assertTrue(cb.mightContain("known"), "remove of unknown must not poison known");
    }

    @Test
    void doubleAddSurvivesSingleRemove() {
        CountingBloomFilter cb = new CountingBloomFilter(100);
        cb.add("key");
        cb.add("key");
        cb.remove("key");
        assertTrue(cb.mightContain("key"), "after add+add+remove key still present");
    }

    @Test
    void emptyFilterRejectsEverything() {
        CountingBloomFilter cb = new CountingBloomFilter(100);
        assertFalse(cb.mightContain("any"));
    }

    @Test
    void saturatedCounterProtectsAgainstOverzealousRemove() {
        CountingBloomFilter cb = new CountingBloomFilter(8);
        for (int i = 0; i < 1000; i++) cb.add("k" + i);
        cb.remove("k0");
        int stillPresent = 0;
        for (int i = 0; i < 1000; i++) if (cb.mightContain("k" + i)) stillPresent++;
        assertTrue(stillPresent >= 950, "stillPresent=" + stillPresent);
    }

    @Test
    void fprTargetHoldsWithDefaultSizing() {
        CountingBloomFilter cb = new CountingBloomFilter(10_000);
        for (int i = 0; i < 10_000; i++) cb.add("present" + i);
        int probes = 100_000;
        int fp = 0;
        for (int i = 0; i < probes; i++) {
            if (cb.mightContain("absent" + i)) fp++;
        }
        double fpr = (double) fp / probes;
        assertTrue(fpr < 0.05, "FPR " + fpr + " exceeded 5%");
    }

    @Test
    void clearEmptiesEveryCounter() {
        CountingBloomFilter cb = new CountingBloomFilter(100);
        for (int i = 0; i < 100; i++) cb.add("key" + i);
        cb.clear();
        for (int i = 0; i < 100; i++) {
            assertFalse(cb.mightContain("key" + i), "cleared filter must reject every key");
        }
    }
}
