package com.submillisecond.recipes.events;

/** What a bounded async dispatcher does when the queue is full. */
public enum OverflowPolicy {
    /** Block the emitter until space frees (back-pressure to the producer). */
    BLOCK,
    /** Drop the incoming event (keep the backlog). */
    DROP_NEWEST,
    /** Evict the oldest queued event to make room (keep the freshest). */
    DROP_OLDEST
}
