package com.submillisecond.recipes.health;

/** Kubernetes-style probe classes. An indicator may serve several. */
public enum ProbeKind {
    LIVENESS,
    READINESS,
    STARTUP
}
