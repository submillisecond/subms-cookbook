package com.submillisecond.recipes.health;

/** Health status taxonomy. Severity: UP < UNKNOWN < WARN < DEGRADED < DOWN. */
public enum HealthStatus {
    UP(0),
    UNKNOWN(1),
    WARN(2),
    DEGRADED(3),
    DOWN(4);

    private final int rank;

    HealthStatus(int rank) {
        this.rank = rank;
    }

    public String token() {
        return name();
    }

    public HealthStatus worse(HealthStatus other) {
        return other.rank > this.rank ? other : this;
    }

    public static HealthStatus aggregate(Iterable<HealthStatus> statuses) {
        HealthStatus acc = UP;
        for (HealthStatus s : statuses) {
            acc = acc.worse(s);
        }
        return acc;
    }

    /** UP/UNKNOWN/WARN -> 200, DEGRADED/DOWN -> 503. */
    public static int httpStatusFor(HealthStatus status) {
        return switch (status) {
            case UP, UNKNOWN, WARN -> 200;
            case DEGRADED, DOWN -> 503;
        };
    }
}
