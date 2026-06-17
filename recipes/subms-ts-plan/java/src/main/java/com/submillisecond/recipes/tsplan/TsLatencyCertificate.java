package com.submillisecond.recipes.tsplan;

import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * A signed-by-checksum latency guarantee composed from a {@link TsPlan}.
 *
 * <p>The certificate carries a deterministic FNV-1a integrity hash over its
 * canonical JSON, so tampering is detectable and the Rust + Java ports agree
 * byte-for-byte. That hash is tamper-evidence, NOT a cryptographic signature; to
 * sign for real, run a signer over {@link #toJson()} (the key is the consumer's,
 * so it stays a pluggable hook).
 */
public final class TsLatencyCertificate {

    private static final long FNV_OFFSET_BASIS = 0xcbf29ce484222325L;
    private static final long FNV_PRIME = 0x100000001b3L;

    private final String hardwareTier;
    private final long totalP99Ns;
    private final long plannerOverheadNs;
    private final long validUntil;
    private final List<TsPlanStage> stages;
    private final long integrity;

    /**
     * Build a certificate and compute its integrity hash. This is the path
     * {@link TsPlan#certify} takes.
     */
    public TsLatencyCertificate(
            String hardwareTier,
            long totalP99Ns,
            long plannerOverheadNs,
            long validUntil,
            List<TsPlanStage> stages) {
        this(hardwareTier, totalP99Ns, plannerOverheadNs, validUntil, stages, true, 0L);
    }

    /**
     * Rebuild a certificate from parsed-out fields with an externally supplied
     * integrity hash, without recomputing it. Mirrors constructing the Rust
     * struct field-by-field; {@link #verify()} then re-derives the hash to
     * confirm the fields are intact.
     */
    public TsLatencyCertificate(
            String hardwareTier,
            long totalP99Ns,
            long plannerOverheadNs,
            long validUntil,
            List<TsPlanStage> stages,
            long integrity) {
        this(hardwareTier, totalP99Ns, plannerOverheadNs, validUntil, stages, false, integrity);
    }

    private TsLatencyCertificate(
            String hardwareTier,
            long totalP99Ns,
            long plannerOverheadNs,
            long validUntil,
            List<TsPlanStage> stages,
            boolean compute,
            long suppliedIntegrity) {
        this.hardwareTier = hardwareTier;
        this.totalP99Ns = totalP99Ns;
        this.plannerOverheadNs = plannerOverheadNs;
        this.validUntil = validUntil;
        this.stages = new ArrayList<>(stages);
        this.integrity = compute ? fnv1a(canonicalBody().getBytes(StandardCharsets.UTF_8)) : suppliedIntegrity;
    }

    public String hardwareTier() {
        return hardwareTier;
    }

    public long totalP99Ns() {
        return totalP99Ns;
    }

    public long plannerOverheadNs() {
        return plannerOverheadNs;
    }

    public long validUntil() {
        return validUntil;
    }

    public List<TsPlanStage> stages() {
        return Collections.unmodifiableList(stages);
    }

    public long integrity() {
        return integrity;
    }

    /** Does the composed p99 fit within {@code budgetNs}? */
    public boolean meetsBudget(long budgetNs) {
        return Long.compareUnsigned(totalP99Ns, budgetNs) <= 0;
    }

    /** Recompute the integrity hash and compare. Detects any field tamper. */
    public boolean verify() {
        return fnv1a(canonicalBody().getBytes(StandardCharsets.UTF_8)) == integrity;
    }

    /**
     * The canonical JSON of every field except {@code integrity} - the bytes the
     * hash is taken over and what a real signer would sign.
     */
    private String canonicalBody() {
        StringBuilder out = new StringBuilder();
        out.append('{');
        out.append("\"hardware_tier\":");
        pushJsonStr(out, hardwareTier);
        out.append(",\"total_p99_ns\":").append(Long.toUnsignedString(totalP99Ns));
        out.append(",\"planner_overhead_ns\":").append(Long.toUnsignedString(plannerOverheadNs));
        out.append(",\"valid_until\":").append(Long.toString(validUntil));
        out.append(",\"stages\":[");
        for (int i = 0; i < stages.size(); i++) {
            if (i > 0) {
                out.append(',');
            }
            TsPlanStage s = stages.get(i);
            out.append("{\"recipe\":");
            pushJsonStr(out, s.recipe());
            out.append(",\"stage\":");
            pushJsonStr(out, s.stage());
            out.append(",\"p99_ns\":").append(Long.toUnsignedString(s.p99Ns())).append('}');
        }
        out.append("]}");
        return out.toString();
    }

    /** Full certificate JSON, including the integrity hash. */
    public String toJson() {
        String body = canonicalBody();
        // splice `,"integrity":N` before the closing brace of the body.
        StringBuilder out = new StringBuilder(body.substring(0, body.length() - 1));
        out.append(",\"integrity\":").append(Long.toUnsignedString(integrity)).append('}');
        return out.toString();
    }

    private static void pushJsonStr(StringBuilder out, String s) {
        out.append('"');
        for (int i = 0; i < s.length(); i++) {
            char c = s.charAt(i);
            switch (c) {
                case '"' -> out.append("\\\"");
                case '\\' -> out.append("\\\\");
                case '\n' -> out.append("\\n");
                case '\r' -> out.append("\\r");
                case '\t' -> out.append("\\t");
                default -> out.append(c);
            }
        }
        out.append('"');
    }

    /**
     * FNV-1a 64-bit. Java longs are signed, but the multiply wraps the same as
     * Rust's {@code wrapping_mul} on u64, so the bit pattern matches and the
     * certificate hash is byte-equivalent on Rust + Java over the same UTF-8
     * bytes.
     */
    private static long fnv1a(byte[] bytes) {
        long h = FNV_OFFSET_BASIS;
        for (byte b : bytes) {
            h ^= (b & 0xffL);
            h *= FNV_PRIME;
        }
        return h;
    }
}
