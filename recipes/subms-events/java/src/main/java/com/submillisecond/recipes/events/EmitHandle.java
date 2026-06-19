package com.submillisecond.recipes.events;

/**
 * A cheap, shareable emitter for producers that don't own the dispatcher (e.g. a
 * background thread). The owner keeps the dispatcher for addListener / stop.
 */
public final class EmitHandle {
    private final EventDispatcher dispatcher;

    EmitHandle(EventDispatcher dispatcher) {
        this.dispatcher = dispatcher;
    }

    public void emit(Event event) {
        dispatcher.dispatch(event);
    }

    public DispatchMode mode() {
        return dispatcher.mode();
    }
}
