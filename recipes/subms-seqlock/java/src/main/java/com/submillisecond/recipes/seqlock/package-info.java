/**
 * Single-writer, many-reader snapshot publication over a fixed image of 64-bit words.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-seqlock">https://www.submillisecond.com/cookbook/recipes/subms-seqlock</a>
 */
package com.submillisecond.recipes.seqlock;
