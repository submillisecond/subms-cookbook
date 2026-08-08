/**
 * Fixed-capacity bump-pointer arena over a byte[] with reset() for per-request reuse.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-arena-allocator">https://www.submillisecond.com/cookbook/recipes/subms-arena-allocator</a>
 */
package com.submillisecond.recipes.arena;
