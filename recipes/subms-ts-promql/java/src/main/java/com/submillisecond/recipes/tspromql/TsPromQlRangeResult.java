package com.submillisecond.recipes.tspromql;

import java.util.List;

/**
 * A range evaluation: the query evaluated at each step across {@code [start,
 * end]}. Each {@link Step} pairs an instant with its result vector.
 */
public final class TsPromQlRangeResult {

    public record Step(long ts, TsPromQlResult result) {}

    private final List<Step> steps;

    public TsPromQlRangeResult(List<Step> steps) {
        this.steps = List.copyOf(steps);
    }

    public List<Step> steps() {
        return steps;
    }

    public int size() {
        return steps.size();
    }

    public boolean isEmpty() {
        return steps.isEmpty();
    }
}
