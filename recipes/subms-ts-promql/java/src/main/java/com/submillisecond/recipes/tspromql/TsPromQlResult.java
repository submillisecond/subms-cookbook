package com.submillisecond.recipes.tspromql;

import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.OptionalDouble;
import java.util.TreeMap;

/**
 * The result of evaluating a query at an instant: a vector of {@link TsSample}.
 * Mirrors the Rust {@code TsPromQlResult}.
 */
public final class TsPromQlResult {

    private final List<TsSample> samples;

    public TsPromQlResult(List<TsSample> samples) {
        this.samples = List.copyOf(samples);
    }

    public List<TsSample> samples() {
        return samples;
    }

    public int size() {
        return samples.size();
    }

    public boolean isEmpty() {
        return samples.isEmpty();
    }

    /** The value of the lone sample whose labels match {@code want} exactly. */
    public OptionalDouble valueFor(Map<String, String> want) {
        TreeMap<String, String> target = new TreeMap<>(want);
        for (TsSample s : samples) {
            if (s.labels().equals(target)) {
                return OptionalDouble.of(s.value());
            }
        }
        return OptionalDouble.empty();
    }

    /** The lone value when the result is a single sample. */
    public Optional<Double> scalar() {
        return samples.size() == 1 ? Optional.of(samples.get(0).value()) : Optional.empty();
    }
}
