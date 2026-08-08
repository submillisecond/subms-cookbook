/**
 * A health endpoint library: indicators with worst-wins aggregation, a redaction-aware env/deploy provider, Kubernetes probe kinds, critical/non-critical demotion, and a background-refreshed cached snapshot. Built on subms-events. Byte-equivalent to the Rust crate subms-health.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-health">https://www.submillisecond.com/cookbook/recipes/subms-health</a>
 */
package com.submillisecond.recipes.health;
