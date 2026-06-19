package com.submillisecond.recipes.health;

/** Framework-agnostic endpoint helper: returns a (code, json) Result. */
public final class HealthEndpoint {
    private HealthEndpoint() {}

    public static HealthRegistry.Result handle(HealthRegistry registry, ProbeKind probe) {
        if (probe == null) {
            return registry.render();
        }
        return switch (probe) {
            case LIVENESS -> registry.renderLiveness();
            case READINESS -> registry.renderReadiness();
            case STARTUP -> registry.renderStartup();
        };
    }
}
