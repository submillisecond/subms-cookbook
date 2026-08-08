/**
 * Pure static methods on long[] sample arrays: percentiles, log2-spaced CDF histograms, jitter score, tail analysis (CTE / Hill index / fatness ratio), robust statistics (IQR / MAD / CoV / skew / kurtosis), KS distribution comparison, Cohen's d effect size, and bootstrap confidence intervals. Zero-dependency. Byte-equivalent to the Rust sibling subms-stats crate.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-stats">https://www.submillisecond.com/cookbook/recipes/subms-stats</a>
 */
package com.submillisecond.recipes.stats;
