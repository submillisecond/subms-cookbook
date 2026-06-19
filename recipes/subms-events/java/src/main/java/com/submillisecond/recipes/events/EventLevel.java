package com.submillisecond.recipes.events;

/** Event severity. The wire token is the UPPERCASE enum name. */
public enum EventLevel {
    TRACE,
    DEBUG,
    INFO,
    WARN,
    ERROR;

    public String token() {
        return name();
    }
}
