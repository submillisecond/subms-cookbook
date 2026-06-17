package com.submillisecond.recipes.tsplan;

/**
 * One step of a plan: a recipe + stage and the published p99 (ns) for it.
 */
public record TsPlanStage(String recipe, String stage, long p99Ns) {
}
