package com.submillisecond.recipes.health;

/** Stdout demo: a registry with system sections + a couple of indicators. */
public final class Demo {
    public static void main(String[] args) {
        HealthRegistry reg = HealthRegistry.withSystemSections();
        reg.registerFn("db", () -> ComponentHealth.up().withDetail("ping", "ok"), new RefreshPolicy());
        reg.registerFn("cache", () -> ComponentHealth.down("connection refused"),
                new RefreshPolicy().critical(false));

        HealthRegistry.Result r = reg.render();
        System.out.println("HTTP " + r.code());
        System.out.println(r.json());
        System.out.println("/health/live -> HTTP " + reg.renderLiveness().code());
    }
}
