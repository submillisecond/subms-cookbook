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
        shardMergeAndOccupancy();
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

    /**
     * Each shard builds its own filter over the symbols it saw, then the gateway
     * ORs them into one membership set. {@code union} only accepts identical
     * geometry, so every shard must be constructed with the same expected count.
     * {@code estimatedFpp} reports occupancy against the design point, which is
     * how you find out a filter has outgrown its sizing before the false
     * positives do.
     */
    static void shardMergeAndOccupancy() {
        System.out.println("\n== merge: per-shard filters unioned at the gateway ==");
        int capacity = 10_000;
        BloomFilter gateway = new BloomFilter(capacity);
        for (int shard = 0; shard < 4; shard++) {
            BloomFilter local = new BloomFilter(capacity);
            for (int i = 0; i < 500; i++) local.add("shard" + shard + "-sym" + i);
            gateway.union(local);
        }
        System.out.println("  merged 4 shards x 500 symbols");
        System.out.println("  approx distinct keys: " + gateway.approximateElementCount());
        System.out.printf("  occupancy fpp:        %.4f%%%n", gateway.estimatedFpp() * 100.0);

        try {
            gateway.union(new BloomFilter(capacity * 2));
            throw new AssertionError("geometry check must reject this");
        } catch (IllegalArgumentException e) {
            System.out.println("  refused mismatched shard: " + e.getMessage());
        }

        gateway.clear();
        System.out.println("  after clear -> approx distinct keys: "
                + gateway.approximateElementCount());
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
