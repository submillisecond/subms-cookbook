/**
 * In-memory event sourcing on subms-events: append-only log, offsets, full replay + incremental projections, and live subscriptions. Byte-equivalent to the Rust crate subms-events-store.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-events-store">https://www.submillisecond.com/cookbook/recipes/subms-events-store</a>
 */
package com.submillisecond.recipes.eventstore;
