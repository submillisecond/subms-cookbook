package com.submillisecond.recipes.events;

import java.util.function.Predicate;

/** Gate: forward to the inner listener only when the predicate passes. */
public final class FilterListener implements EventListener {
    private final Predicate<Event> predicate;
    private final EventListener inner;

    public FilterListener(Predicate<Event> predicate, EventListener inner) {
        this.predicate = predicate;
        this.inner = inner;
    }

    @Override
    public void onEvent(Event event) {
        if (predicate.test(event)) {
            inner.onEvent(event);
        }
    }
}
