package com.submillisecond.recipes.cuckoo.features;

import com.submillisecond.recipes.cuckoo.CuckooFilter;
import java.util.ArrayList;
import java.util.List;

/**
 * Dynamic cuckoo filter: chains a fresh cuckoo filter at each load
 * milestone so the structure grows past its initial sizing without
 * the rejection-at-saturation behaviour of the base filter.
 *
 * <p>Algorithm (Chen et al., "Dynamic Cuckoo Filter", 2017): when the
 * active filter's load factor passes {@code growThreshold} (default
 * 0.95), allocate a new filter at double the bucket count and start
 * inserting there. Membership query asks every filter; positive if
 * ANY layer says yes. Delete probes every layer in newest-first order
 * and removes from the first match.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_cuckoo_filter::features::dynamic::DynamicCuckooFilter}.
 */
public final class DynamicCuckooFilter {

    private static final double DEFAULT_GROW_THRESHOLD = 0.95;
    private static final int GROWTH_FACTOR = 2;
    private static final int DEFAULT_INITIAL_BUCKETS_HINT = 1024;
    private static final int BUCKET_SIZE = 4;

    private final List<CuckooFilter> layers = new ArrayList<>();
    private final List<Integer> layerCapacities = new ArrayList<>();
    private final double growThreshold;

    public DynamicCuckooFilter(int initialCapacity) {
        this(initialCapacity, DEFAULT_GROW_THRESHOLD);
    }

    public DynamicCuckooFilter(int initialCapacity, double growThreshold) {
        int cap = Math.max(initialCapacity, DEFAULT_INITIAL_BUCKETS_HINT / BUCKET_SIZE);
        double t = (Double.isFinite(growThreshold) && growThreshold > 0.0 && growThreshold < 1.0)
                ? growThreshold
                : DEFAULT_GROW_THRESHOLD;
        this.growThreshold = t;
        this.layers.add(new CuckooFilter(cap));
        this.layerCapacities.add(cap);
    }

    public int layerCount() { return layers.size(); }

    public int size() {
        int sum = 0;
        for (CuckooFilter l : layers) sum += l.size();
        return sum;
    }

    public boolean isEmpty() { return size() == 0; }

    public boolean insert(String key) {
        if (shouldGrow()) grow();
        CuckooFilter active = layers.get(layers.size() - 1);
        if (active.insert(key)) return true;
        // Saturation at the active layer despite the threshold check:
        // grow once more and retry. Bounded by total len() so it
        // terminates even under pathological collisions.
        grow();
        return layers.get(layers.size() - 1).insert(key);
    }

    public boolean contains(String key) {
        for (CuckooFilter l : layers) {
            if (l.contains(key)) return true;
        }
        return false;
    }

    public boolean delete(String key) {
        for (int i = layers.size() - 1; i >= 0; i--) {
            if (layers.get(i).delete(key)) return true;
        }
        return false;
    }

    public double loadFactor() {
        int last = layers.size() - 1;
        int cap = layerCapacities.get(last);
        if (cap == 0) return 0.0;
        return (double) layers.get(last).size() / cap;
    }

    double growThresholdForTest() { return growThreshold; }

    private boolean shouldGrow() {
        int last = layers.size() - 1;
        int cap = layerCapacities.get(last);
        if (cap == 0) return false;
        return (double) layers.get(last).size() / cap >= growThreshold;
    }

    private void grow() {
        int last = layerCapacities.size() - 1;
        int newCap = layerCapacities.get(last) * GROWTH_FACTOR;
        layers.add(new CuckooFilter(newCap));
        layerCapacities.add(newCap);
    }
}
