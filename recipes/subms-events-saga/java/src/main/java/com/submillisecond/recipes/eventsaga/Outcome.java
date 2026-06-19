package com.submillisecond.recipes.eventsaga;

/** Overall result of a saga run. */
public enum Outcome {
    COMMITTED,
    COMPENSATED;

    public String token() {
        return name();
    }
}
