package com.submillisecond.recipes.events;

import java.util.Collections;
import java.util.Map;
import java.util.Objects;
import java.util.SortedMap;
import java.util.TreeMap;

/**
 * A structured, immutable event: a topic, a level, an optional timestamp +
 * message, and a sorted string attribute map. JSON is hand-built and
 * deterministic - byte-equivalent to the Rust + Python ports.
 */
public final class Event {
    private final String topic;
    private final EventLevel level;
    private final String at;
    private final String message; // nullable
    private final SortedMap<String, String> attributes;

    Event(String topic, EventLevel level, String at, String message, SortedMap<String, String> attributes) {
        this.topic = topic;
        this.level = level;
        this.at = at;
        this.message = message;
        this.attributes = Collections.unmodifiableSortedMap(attributes);
    }

    public static EventBuilder builder(String topic) {
        return new EventBuilder(topic);
    }

    /** The common "X moved A -> B" shape: topic + scope/from/to attributes. */
    public static Event transition(String topic, EventLevel level, String scope, String from, String to) {
        return builder(topic).level(level).attr("scope", scope).attr("from", from).attr("to", to).build();
    }

    public String topic() {
        return topic;
    }

    public EventLevel level() {
        return level;
    }

    public String at() {
        return at;
    }

    public String message() {
        return message;
    }

    public Map<String, String> attributes() {
        return attributes;
    }

    public String attr(String key) {
        return attributes.get(key);
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder();
        sb.append("{\"topic\":");
        Json.escape(sb, topic);
        sb.append(",\"level\":");
        Json.escape(sb, level.token());
        sb.append(",\"at\":");
        Json.escape(sb, at);
        if (message != null) {
            sb.append(",\"message\":");
            Json.escape(sb, message);
        }
        if (!attributes.isEmpty()) {
            sb.append(",\"attributes\":{");
            boolean first = true;
            for (Map.Entry<String, String> e : attributes.entrySet()) {
                if (!first) {
                    sb.append(',');
                }
                first = false;
                Json.escape(sb, e.getKey());
                sb.append(':');
                Json.escape(sb, e.getValue());
            }
            sb.append('}');
        }
        sb.append('}');
        return sb.toString();
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) {
            return true;
        }
        if (!(o instanceof Event other)) {
            return false;
        }
        return topic.equals(other.topic)
                && level == other.level
                && at.equals(other.at)
                && Objects.equals(message, other.message)
                && attributes.equals(other.attributes);
    }

    @Override
    public int hashCode() {
        return Objects.hash(topic, level, at, message, attributes);
    }
}
