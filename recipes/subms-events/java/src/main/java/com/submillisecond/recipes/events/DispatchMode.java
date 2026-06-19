package com.submillisecond.recipes.events;

/** How listeners are invoked. */
public enum DispatchMode {
    /** Inline on the emitting thread - no dispatcher thread, no queue. */
    SYNC,
    /** On a dedicated dispatcher thread (default). */
    ASYNC
}
