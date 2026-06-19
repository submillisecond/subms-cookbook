package com.submillisecond.recipes.events;

/** An external event sink. Adapters (e.g. subms-otel) implement it. */
public interface EventBridge {
    String name();

    void forward(Event event);

    /** Optional hook for buffered bridges. */
    default void flush() {}
}
