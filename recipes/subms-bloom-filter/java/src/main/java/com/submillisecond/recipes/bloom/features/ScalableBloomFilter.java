package com.submillisecond.recipes.bloom.features;

import java.util.ArrayList;
import java.util.List;

/**
 * Scalable bloom filter: tier of {@link CountingBloomFilter} layers
 * that adds a new (larger) layer when the active layer saturates.
 * Holds a target FPR as cardinality grows beyond the initial sizing.
 *
 * <p>Algorithm (Almeida et al., "Scalable Bloom Filters"):
 * <ol>
 *   <li>Start with one layer sized for {@code initialCapacity} entries.</li>
 *   <li>When the active layer reaches {@code initialCapacity} entries,
 *       add a new layer with {@code growthFactor}x the capacity.</li>
 *   <li>Membership query asks every layer; positive if ANY layer says yes.</li>
 * </ol>
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_bloom_filter::ScalableBloomFilter}.
 */
public final class ScalableBloomFilter {

    private final List<CountingBloomFilter> layers = new ArrayList<>();
    private final List<Integer> layerCapacities = new ArrayList<>();
    private final List<Integer> layerCounts = new ArrayList<>();
    private final int growthFactor;

    public ScalableBloomFilter(int initialCapacity) {
        this(initialCapacity, 2);
    }

    public ScalableBloomFilter(int initialCapacity, int growthFactor) {
        int cap = Math.max(1, initialCapacity);
        int g = Math.max(2, growthFactor);
        this.growthFactor = g;
        this.layers.add(new CountingBloomFilter(cap));
        this.layerCapacities.add(cap);
        this.layerCounts.add(0);
    }

    public void add(String key) {
        int lastIdx = layers.size() - 1;
        if (layerCounts.get(lastIdx) >= layerCapacities.get(lastIdx)) {
            addLayer();
        }
        int idx = layers.size() - 1;
        layers.get(idx).add(key);
        layerCounts.set(idx, layerCounts.get(idx) + 1);
    }

    public boolean mightContain(String key) {
        for (CountingBloomFilter l : layers) {
            if (l.mightContain(key)) return true;
        }
        return false;
    }

    public int layerCount() { return layers.size(); }

    public int totalCount() {
        int sum = 0;
        for (int c : layerCounts) sum += c;
        return sum;
    }

    private void addLayer() {
        int last = layerCapacities.size() - 1;
        int newCap = layerCapacities.get(last) * growthFactor;
        layers.add(new CountingBloomFilter(newCap));
        layerCapacities.add(newCap);
        layerCounts.add(0);
    }
}
