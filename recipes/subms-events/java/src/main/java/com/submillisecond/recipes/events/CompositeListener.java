package com.submillisecond.recipes.events;

import java.util.ArrayList;
import java.util.List;

/** Fan-out: deliver each event to several listeners in order. */
public final class CompositeListener implements EventListener {
    private final List<EventListener> listeners;

    public CompositeListener(List<EventListener> listeners) {
        this.listeners = new ArrayList<>(listeners);
    }

    public CompositeListener push(EventListener listener) {
        listeners.add(listener);
        return this;
    }

    @Override
    public void onEvent(Event event) {
        for (EventListener l : listeners) {
            l.onEvent(event);
        }
    }
}
