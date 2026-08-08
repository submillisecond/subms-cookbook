/**
 * Distinct-count cardinality estimator; ~1% standard error at ~16 KB.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-hyperloglog">https://www.submillisecond.com/cookbook/recipes/subms-hyperloglog</a>
 */
package com.submillisecond.recipes.hll;
