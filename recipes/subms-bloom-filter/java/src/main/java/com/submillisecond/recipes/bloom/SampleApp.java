package com.submillisecond.recipes.bloom;

import com.submillisecond.recipes.bloom.features.CountingBloomFilter;
import com.submillisecond.recipes.bloom.features.PartitionedBloomFilter;
import com.submillisecond.recipes.bloom.features.ScalableBloomFilter;

/**
 * Sample app: a tour of {@code subms-bloom-filter}, base API first, then each
 * optional variant. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.bloom.SampleApp}
 *
 * <ul>
 *   <li>base        - URL-seen dedup for a web crawler's frontier
 *   <li>counting    - an active-session set that supports removal (logout)
 *   <li>scalable    - a filter that grows in layers as it fills, keeping FPR bounded
 *   <li>partitioned - the independent-slice variant
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) {
        baseCrawlerDedup();
        countingSessionSet();
        scalableGrowth();
        partitionedVariant();
    }

    /** Base API: a crawler skips URLs it has already fetched. */
    static void baseCrawlerDedup() {
        System.out.println("== base: crawler URL dedup ==");
        BloomFilter seen = new BloomFilter(10_000);
        String[] frontier = {
            "https://a.example/", "https://b.example/", "https://a.example/",
            "https://c.example/", "https://b.example/"
        };
        int fetched = 0, skipped = 0;
        for (String url : frontier) {
            if (seen.mightContain(url)) {
                System.out.println("  skip  " + url);
                skipped++;
            } else {
                seen.add(url);
                System.out.println("  fetch " + url);
                fetched++;
            }
        }
        System.out.println("  -> fetched " + fetched + ", skipped " + skipped);
        for (String url : new String[] {"https://a.example/", "https://b.example/", "https://c.example/"}) {
            if (!seen.mightContain(url)) throw new AssertionError("no false negatives");
        }
    }

    /** counting: a counting bloom filter supports removal (a plain one cannot). */
    static void countingSessionSet() {
        System.out.println("\n== counting: active sessions with logout ==");
        CountingBloomFilter sessions = new CountingBloomFilter(1_000);
        for (String s : new String[] {"sess-alice", "sess-bob", "sess-carol"}) sessions.add(s);
        System.out.println("  bob active?   " + sessions.mightContain("sess-bob"));
        sessions.remove("sess-bob"); // logout
        System.out.println("  bob after logout? " + sessions.mightContain("sess-bob"));
        if (!sessions.mightContain("sess-alice")) throw new AssertionError("other sessions untouched");
    }

    /** scalable: grows a fresh layer as it fills, so FPR stays bounded with no count known up front. */
    static void scalableGrowth() {
        System.out.println("\n== scalable: grows past its initial capacity ==");
        ScalableBloomFilter f = new ScalableBloomFilter(64);
        for (int i = 0; i < 1_000; i++) f.add("key-" + i);
        System.out.println("  added 1000 into a cap-64 filter -> " + f.layerCount() + " layers");
        if (f.layerCount() <= 1) throw new AssertionError("it grew");
        for (int i = 0; i < 1_000; i++) {
            if (!f.mightContain("key-" + i)) throw new AssertionError("no false negatives after growth");
        }
    }

    /** partitioned: each hash owns its own equal slice, making the FPR easy to reason about. */
    static void partitionedVariant() {
        System.out.println("\n== partitioned: one slice per hash ==");
        PartitionedBloomFilter f = new PartitionedBloomFilter(1_000);
        for (String tag : new String[] {"red", "green", "blue"}) f.add(tag);
        System.out.println("  " + f.bitCount() + " bits across " + f.k() + " slices");
        System.out.println("  green present? " + f.mightContain("green"));
        if (!f.mightContain("red")) throw new AssertionError("stored tag must be present");
    }
}
