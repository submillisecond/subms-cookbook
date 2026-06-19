package com.submillisecond.recipes.eventstore;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.events.DispatchMode;
import com.submillisecond.recipes.events.Event;
import com.submillisecond.recipes.events.EventDispatcher;
import com.submillisecond.recipes.events.EventListener;

/**
 * In-memory, append-only log of events with offset addressing and live
 * subscriptions. Durability is out of scope - pair with subms-ts-wal to persist.
 */
public final class EventStore {
    private final List<Event> log = new ArrayList<>();
    private final EventDispatcher dispatcher;

    public EventStore() {
        this(DispatchMode.SYNC);
    }

    public EventStore(DispatchMode dispatch) {
        this.dispatcher = new EventDispatcher(dispatch);
    }

    /** Append an event; returns its 0-based offset. Subscribers are notified. */
    public long append(Event event) {
        long offset = log.size();
        log.add(event);
        dispatcher.emit(event);
        return offset;
    }

    public int size() {
        return log.size();
    }

    public boolean isEmpty() {
        return log.isEmpty();
    }

    public Optional<Event> get(long offset) {
        if (offset >= 0 && offset < log.size()) {
            return Optional.of(log.get((int) offset));
        }
        return Optional.empty();
    }

    public List<Event> events() {
        return log;
    }

    public List<Event> readFrom(long offset) {
        int i = (int) Math.min(offset, log.size());
        return new ArrayList<>(log.subList(i, log.size()));
    }

    public List<Event> byTopic(String topic) {
        List<Event> out = new ArrayList<>();
        for (Event e : log) {
            if (e.topic().equals(topic)) {
                out.add(e);
            }
        }
        return out;
    }

    public void subscribe(EventListener listener) {
        dispatcher.addListener(listener);
    }

    public void stop() {
        dispatcher.stop();
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder("[");
        for (int i = 0; i < log.size(); i++) {
            if (i > 0) {
                sb.append(',');
            }
            sb.append(log.get(i).toJson());
        }
        return sb.append(']').toString();
    }
}
