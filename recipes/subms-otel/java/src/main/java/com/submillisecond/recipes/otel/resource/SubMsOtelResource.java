package com.submillisecond.recipes.otel.resource;

import io.opentelemetry.api.common.AttributeKey;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.common.AttributesBuilder;
import io.opentelemetry.sdk.resources.Resource;

import java.util.UUID;

/**
 * Resource semconv detector. Populates an OpenTelemetry {@link Resource} with the standard
 * {@code service.*}, {@code host.*}, {@code os.*}, {@code process.runtime.*}, {@code subms.*}, and
 * {@code cloud.provider} attributes. Used by autoconfig and exposed for callers wiring providers by hand.
 */
public final class SubMsOtelResource {

    public static final String DEFAULT_SERVICE_NAME = "subms";

    private SubMsOtelResource() {}

    public static Resource detect() {
        AttributesBuilder b = Attributes.builder();
        b.put(AttributeKey.stringKey("service.name"), serviceName());
        b.put(AttributeKey.stringKey("service.version"), serviceVersion());
        b.put(AttributeKey.stringKey("service.instance.id"), UUID.randomUUID().toString());

        String host = detectHostName();
        if (host != null) {
            b.put(AttributeKey.stringKey("host.name"), host);
        }
        b.put(AttributeKey.stringKey("host.arch"), System.getProperty("os.arch", "unknown"));
        b.put(AttributeKey.stringKey("os.type"), normalizeOsType(System.getProperty("os.name", "")));
        String osVersion = System.getProperty("os.version");
        if (osVersion != null && !osVersion.isEmpty()) {
            b.put(AttributeKey.stringKey("os.version"), osVersion);
        }

        b.put(AttributeKey.stringKey("process.runtime.name"),
                System.getProperty("java.runtime.name", "OpenJDK"));
        b.put(AttributeKey.stringKey("process.runtime.version"),
                System.getProperty("java.version", "unknown"));

        String submsHost = env("SUBMS_HOST");
        if (submsHost != null) {
            b.put(AttributeKey.stringKey("subms.host"), submsHost);
        }
        String tier = env("SUBMS_HARDWARE_TIER");
        if (tier != null) {
            b.put(AttributeKey.stringKey("subms.hardware.tier"), tier);
        }

        String cloud = detectCloudProvider();
        if (cloud != null) {
            b.put(AttributeKey.stringKey("cloud.provider"), cloud);
        }

        String extra = env("OTEL_RESOURCE_ATTRIBUTES");
        if (extra != null) {
            for (String entry : extra.split(",")) {
                int eq = entry.indexOf('=');
                if (eq <= 0 || eq == entry.length() - 1) continue;
                String k = entry.substring(0, eq).trim();
                String v = entry.substring(eq + 1).trim();
                if (!k.isEmpty() && !v.isEmpty()) {
                    b.put(AttributeKey.stringKey(k), v);
                }
            }
        }

        return Resource.create(b.build());
    }

    private static String serviceName() {
        String v = env("OTEL_SERVICE_NAME");
        return (v != null) ? v : DEFAULT_SERVICE_NAME;
    }

    private static String serviceVersion() {
        String v = env("OTEL_SERVICE_VERSION");
        if (v != null) return v;
        Package pkg = SubMsOtelResource.class.getPackage();
        if (pkg != null) {
            String implVersion = pkg.getImplementationVersion();
            if (implVersion != null && !implVersion.isEmpty()) return implVersion;
        }
        return "unknown";
    }

    private static String detectHostName() {
        String submsHost = env("SUBMS_HOST");
        if (submsHost != null) return submsHost;
        String hostname = env("HOSTNAME");
        if (hostname != null) return hostname;
        String computerName = env("COMPUTERNAME");
        if (computerName != null) return computerName;
        try {
            return java.net.InetAddress.getLocalHost().getHostName();
        } catch (Exception ignored) {
            return null;
        }
    }

    private static String detectCloudProvider() {
        if (env("AWS_REGION") != null) return "aws";
        if (env("GCP_PROJECT") != null || env("GOOGLE_CLOUD_PROJECT") != null) return "gcp";
        if (env("AZURE_REGION") != null) return "azure";
        return null;
    }

    private static String normalizeOsType(String osName) {
        String lower = osName.toLowerCase();
        if (lower.contains("win")) return "windows";
        if (lower.contains("mac") || lower.contains("darwin")) return "darwin";
        if (lower.contains("linux")) return "linux";
        if (lower.contains("freebsd")) return "freebsd";
        return lower.isEmpty() ? "unknown" : lower;
    }

    private static String env(String name) {
        String v = System.getenv(name);
        if (v == null || v.isEmpty()) return null;
        return v;
    }
}
