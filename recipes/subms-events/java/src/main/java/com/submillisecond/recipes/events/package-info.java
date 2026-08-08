/**
 * A low-latency in-process event system: structured Event + builder, sync-inline / async-off-thread dispatcher, fn/composite/filter listeners, and an EventBridge sink interface. Byte-equivalent to the Rust crate subms-events.
 *
 * <p>Full writeup, design notes and measured benchmarks:
 * <a href="https://www.submillisecond.com/cookbook/recipes/subms-events">https://www.submillisecond.com/cookbook/recipes/subms-events</a>
 */
package com.submillisecond.recipes.events;
