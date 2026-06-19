package com.submillisecond.recipes.health;

import java.util.Arrays;
import java.util.List;

/** Per-indicator policy: cadence, probe kinds, and whether a failure is critical. */
public final class RefreshPolicy {
    private long intervalMs = 30_000;
    private List<ProbeKind> probeKinds = List.of(ProbeKind.READINESS);
    private boolean critical = true;

    public RefreshPolicy intervalMs(long ms) {
        this.intervalMs = ms;
        return this;
    }

    public RefreshPolicy kinds(ProbeKind... kinds) {
        this.probeKinds = Arrays.asList(kinds);
        return this;
    }

    public RefreshPolicy critical(boolean critical) {
        this.critical = critical;
        return this;
    }

    public long intervalMs() {
        return intervalMs;
    }

    public boolean isCritical() {
        return critical;
    }

    public boolean includes(ProbeKind kind) {
        return probeKinds.contains(kind);
    }
}
