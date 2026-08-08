/**
 * Lock-free GCRA rate limiter using a single-atomic CAS-loop.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-rate-limiter">https://www.submillisecond.com/cookbook/recipes/subms-rate-limiter</a>
 */
package com.submillisecond.recipes.ratelimit;
