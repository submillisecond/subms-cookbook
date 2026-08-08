/**
 * Wait-free SPSC ring buffer with cache-line padded counters and opposite-index caching.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-spsc-ring-buffer">https://www.submillisecond.com/cookbook/recipes/subms-spsc-ring-buffer</a>
 */
package com.submillisecond.recipes.spsc;
