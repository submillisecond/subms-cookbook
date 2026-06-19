package com.submillisecond.recipes.health;

import java.util.Map;
import java.util.TreeMap;

/** One node in the health tree: status + sorted details + nested components. */
public final class ComponentHealth {
    private HealthStatus status;
    private final TreeMap<String, Object> details = new TreeMap<>();
    private final TreeMap<String, ComponentHealth> components = new TreeMap<>();

    public ComponentHealth(HealthStatus status) {
        this.status = status;
    }

    public static ComponentHealth up() {
        return new ComponentHealth(HealthStatus.UP);
    }

    public static ComponentHealth unknown() {
        return new ComponentHealth(HealthStatus.UNKNOWN);
    }

    public static ComponentHealth down(String reason) {
        ComponentHealth c = new ComponentHealth(HealthStatus.DOWN);
        c.details.put("error", reason);
        return c;
    }

    public static ComponentHealth degraded(String reason) {
        ComponentHealth c = new ComponentHealth(HealthStatus.DEGRADED);
        c.details.put("error", reason);
        return c;
    }

    public ComponentHealth withDetail(String key, Object value) {
        details.put(key, value);
        return this;
    }

    public ComponentHealth withSubcomponent(String name, ComponentHealth child) {
        components.put(name, child);
        return this;
    }

    public HealthStatus status() {
        return status;
    }

    public Map<String, Object> details() {
        return details;
    }

    public Map<String, ComponentHealth> components() {
        return components;
    }

    public HealthStatus effectiveStatus() {
        HealthStatus acc = status;
        for (ComponentHealth c : components.values()) {
            acc = acc.worse(c.effectiveStatus());
        }
        return acc;
    }

    void writeJson(StringBuilder sb) {
        sb.append("{\"status\":");
        Json.escape(sb, status.token());
        if (!details.isEmpty()) {
            sb.append(",\"details\":");
            Json.map(sb, details);
        }
        if (!components.isEmpty()) {
            sb.append(",\"components\":{");
            boolean first = true;
            for (Map.Entry<String, ComponentHealth> e : components.entrySet()) {
                if (!first) {
                    sb.append(',');
                }
                first = false;
                Json.escape(sb, e.getKey());
                sb.append(':');
                e.getValue().writeJson(sb);
            }
            sb.append('}');
        }
        sb.append('}');
    }

    public String toJson() {
        StringBuilder sb = new StringBuilder();
        writeJson(sb);
        return sb.toString();
    }
}
