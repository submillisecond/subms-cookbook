/**
 * Exact fixed-point arithmetic for prices, quantities and notionals: a long at nine decimals,
 * 128-bit widening through multiply and divide, seven rounding modes, tick-grid rounding and
 * scale conversion at the wire boundary.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-fixed-point">https://www.submillisecond.com/cookbook/recipes/subms-fixed-point</a>
 */
package com.submillisecond.recipes.fixedpoint;
