package com.submillisecond.recipes.health;

/** A synchronous health probe. Probed off the request path by the registry. */
public interface HealthIndicator {
    String name();

    ComponentHealth check();

    /** Build a closure indicator: {@code HealthIndicator.of("db", () -> ...)}. */
    static HealthIndicator of(String name, java.util.function.Supplier<ComponentHealth> fn) {
        return new HealthIndicator() {
            @Override
            public String name() {
                return name;
            }

            @Override
            public ComponentHealth check() {
                return fn.get();
            }
        };
    }
}
