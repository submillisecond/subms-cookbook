package com.submillisecond.recipes.events;

/** Receives events. A functional interface, so a lambda is a listener. */
@FunctionalInterface
public interface EventListener {
    void onEvent(Event event);
}
