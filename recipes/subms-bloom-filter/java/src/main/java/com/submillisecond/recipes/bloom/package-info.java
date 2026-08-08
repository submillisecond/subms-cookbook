/**
 * A tiny zero-dependency bloom filter (FNV-1a + double hashing, ~10 bits/key, k=7). Reusable component, pairs with subms-lsm-tree.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-bloom-filter">https://www.submillisecond.com/cookbook/recipes/subms-bloom-filter</a>
 */
package com.submillisecond.recipes.bloom;
