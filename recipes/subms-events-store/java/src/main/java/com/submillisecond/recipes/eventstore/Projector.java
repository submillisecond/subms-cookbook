package com.submillisecond.recipes.eventstore;

import java.util.function.BiFunction;

import com.submillisecond.recipes.events.Event;

/**
 * Incremental projection: holds the materialized state + the next offset.
 * {@link #catchUp} is the sub-ms path - it applies only the tail. The fold
 * function takes (state, event) and returns the new state.
 */
public final class Projector<S> {
    private S state;
    private long next;

    public Projector(S initial) {
        this.state = initial;
        this.next = 0;
    }

    public S state() {
        return state;
    }

    public long position() {
        return next;
    }

    public S catchUp(EventStore store, BiFunction<S, Event, S> apply) {
        for (Event e : store.readFrom(next)) {
            state = apply.apply(state, e);
        }
        next = store.size();
        return state;
    }

    /** Full fold over every event in the store. */
    public static <S> S replay(EventStore store, S initial, BiFunction<S, Event, S> apply) {
        S state = initial;
        for (Event e : store.events()) {
            state = apply.apply(state, e);
        }
        return state;
    }
}
