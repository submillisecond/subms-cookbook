package com.submillisecond.recipes.health;

/** Built-in indicator: pid, hostname, and process uptime. */
public final class ServerIndicator implements HealthIndicator {
    private final long startNanos = System.nanoTime();
    private final long pid = ProcessHandle.current().pid();
    private final String hostname = resolveHostname();

    private static String resolveHostname() {
        String h = System.getenv("HOSTNAME");
        if (h == null || h.isEmpty()) {
            h = System.getenv("COMPUTERNAME");
        }
        if (h == null || h.isEmpty()) {
            try {
                h = java.net.InetAddress.getLocalHost().getHostName();
            } catch (Exception e) {
                h = "unknown";
            }
        }
        return h;
    }

    @Override
    public String name() {
        return "server";
    }

    @Override
    public ComponentHealth check() {
        long uptimeMs = (System.nanoTime() - startNanos) / 1_000_000L;
        return ComponentHealth.up()
                .withDetail("pid", pid)
                .withDetail("hostname", hostname)
                .withDetail("uptime_ms", uptimeMs);
    }
}
