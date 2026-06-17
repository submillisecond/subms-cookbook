package com.submillisecond.recipes.tspromql;

import java.util.Map;
import java.util.Objects;
import java.util.TreeMap;

/**
 * A single result sample: the surviving label set paired with its value at the
 * eval instant. PromQL's instant-vector element. Labels are an ordered
 * {@code TreeMap} so equality + grouping are deterministic across runs.
 */
public final class TsSample {

    private final TreeMap<String, String> labels;
    private final double value;

    public TsSample(Map<String, String> labels, double value) {
        this.labels = new TreeMap<>(labels);
        this.value = value;
    }

    public Map<String, String> labels() {
        return labels;
    }

    public double value() {
        return value;
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof TsSample s)) {
            return false;
        }
        return Double.compare(value, s.value) == 0 && labels.equals(s.labels);
    }

    @Override
    public int hashCode() {
        return Objects.hash(labels, value);
    }

    @Override
    public String toString() {
        return labels + " => " + value;
    }
}
