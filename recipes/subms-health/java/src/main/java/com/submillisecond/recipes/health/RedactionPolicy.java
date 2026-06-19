package com.submillisecond.recipes.health;

/** How to mask a matched secret. Hash/Fingerprint use FNV-1a, matching the ports. */
public enum RedactionPolicy {
    MASK,
    LAST4,
    HASH,
    FINGERPRINT;

    public String apply(String value) {
        return switch (this) {
            case MASK -> "***";
            case LAST4 -> value.length() > 4 ? "***" + value.substring(value.length() - 4) : "***";
            case HASH -> String.format("fnv1a:%016x", fnv1a(value));
            case FINGERPRINT -> String.format("fp_%06x", fnv1a(value) & 0xFFFFFFL);
        };
    }

    static long fnv1a(String s) {
        long h = 0xcbf29ce484222325L;
        byte[] bytes = s.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= 0x100000001b3L;
        }
        return h;
    }
}
