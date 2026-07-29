package com.submillisecond.recipes.bloom;

import com.submillisecond.recipes.bloom.features.CountingBloomFilter;
import com.submillisecond.recipes.bloom.features.PartitionedBloomFilter;
import com.submillisecond.recipes.bloom.features.ScalableBloomFilter;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        BloomFilter bf = new BloomFilter(10_000);
        bf.add("alice");
        assertTrue(bf.mightContain("alice"));   // stored keys always report present
        assertFalse(bf.mightContain("bob"));    // absent keys usually report absent
        // quickstart:end
    }

    @Test
    void crawlerDedupScenario() {
        BloomFilter seen = new BloomFilter(10_000);
        String[] frontier = {
            "https://a.example/", "https://b.example/", "https://a.example/",
            "https://c.example/", "https://b.example/"
        };
        int fetched = 0, skipped = 0;
        for (String url : frontier) {
            if (seen.mightContain(url)) skipped++;
            else { seen.add(url); fetched++; }
        }
        assertEquals(3, fetched, "three distinct URLs fetched");
        assertEquals(2, skipped, "two duplicates skipped");
        for (String url : new String[] {"https://a.example/", "https://b.example/", "https://c.example/"}) {
            assertTrue(seen.mightContain(url), "a stored URL must always report present");
        }
    }

    @Test
    void countingSupportsRemoval() {
        CountingBloomFilter s = new CountingBloomFilter(1_000);
        s.add("sess-bob");
        assertTrue(s.mightContain("sess-bob"));
        s.remove("sess-bob");
        assertFalse(s.mightContain("sess-bob"), "removal clears membership");
    }

    @Test
    void scalableGrowsAndKeepsMembers() {
        ScalableBloomFilter f = new ScalableBloomFilter(64);
        for (int i = 0; i < 1_000; i++) f.add("key-" + i);
        assertTrue(f.layerCount() > 1, "grew past the initial layer");
        for (int i = 0; i < 1_000; i++) {
            assertTrue(f.mightContain("key-" + i), "no false negatives after growth");
        }
    }

    @Test
    void partitionedMembership() {
        PartitionedBloomFilter f = new PartitionedBloomFilter(1_000);
        f.add("red");
        assertTrue(f.mightContain("red"));
    }
}
