package com.submillisecond.recipes.events;

import java.util.TreeMap;

/** Fluent builder for {@link Event}. */
public final class EventBuilder {
    private final String topic;
    private EventLevel level = EventLevel.INFO;
    private String at = "";
    private String message; // nullable
    private final TreeMap<String, String> attributes = new TreeMap<>();

    EventBuilder(String topic) {
        this.topic = topic;
    }

    public EventBuilder level(EventLevel level) {
        this.level = level;
        return this;
    }

    public EventBuilder at(String at) {
        this.at = at;
        return this;
    }

    public EventBuilder message(String message) {
        this.message = message;
        return this;
    }

    public EventBuilder attr(String key, String value) {
        this.attributes.put(key, value);
        return this;
    }

    public Event build() {
        return new Event(topic, level, at, message, new TreeMap<>(attributes));
    }
}
