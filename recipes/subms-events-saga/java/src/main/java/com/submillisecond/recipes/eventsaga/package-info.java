/**
 * In-process compensating-step (saga) executor on subms-events: forward steps with reverse-order rollback on failure, plus step lifecycle events. Byte-equivalent to the Rust crate subms-events-saga.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-events-saga">https://www.submillisecond.com/cookbook/recipes/subms-events-saga</a>
 */
package com.submillisecond.recipes.eventsaga;
