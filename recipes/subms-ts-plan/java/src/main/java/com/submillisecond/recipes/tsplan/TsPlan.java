package com.submillisecond.recipes.tsplan;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;

/**
 * An ordered sequence of recipe calls plus a flat planner overhead.
 *
 * <p>Every recipe in the cookbook arc asserts a tail-latency budget per stage.
 * On their own those are point facts. A query that prunes with a zone map,
 * decodes a Gorilla block, scans a range, then reads a t-digest quantile runs
 * all four in sequence, so its system p99 is the sum of the constituent p99s
 * plus a planner overhead. {@code TsPlan} adds them up; {@link #certify} emits a
 * {@link TsLatencyCertificate} an SRE can put in an SLA.
 *
 * <p>The p99 values are treated as unsigned 64-bit, matching the Rust crate's
 * {@code u64}. The total uses saturating addition so a pathological plan reports
 * the unsigned ceiling ({@code -1L}, i.e. {@code 0xffff_ffff_ffff_ffff}) rather
 * than wrapping.
 */
public final class TsPlan {

    private final List<TsPlanStage> stages = new ArrayList<>();
    private long plannerOverheadNs;

    public TsPlan() {
    }

    /** Append a stage citing a recipe's published p99 (builder style). */
    public TsPlan then(String recipe, String stage, long p99Ns) {
        stages.add(new TsPlanStage(recipe, stage, p99Ns));
        return this;
    }

    /** Flat overhead the planner adds on top of the constituent stages. */
    public TsPlan withOverhead(long ns) {
        this.plannerOverheadNs = ns;
        return this;
    }

    public List<TsPlanStage> stages() {
        return Collections.unmodifiableList(stages);
    }

    public long plannerOverheadNs() {
        return plannerOverheadNs;
    }

    /**
     * Composed system p99: the sum of stage p99s plus the planner overhead.
     * Unsigned saturating so a pathological plan reports the unsigned max rather
     * than wrap.
     */
    public long totalP99Ns() {
        long acc = plannerOverheadNs;
        for (TsPlanStage s : stages) {
            acc = saturatingAddUnsigned(acc, s.p99Ns());
        }
        return acc;
    }

    /**
     * Freeze the plan into a certificate for {@code hardwareTier}, valid until
     * the given epoch-nanos deadline (0 = unbounded).
     */
    public TsLatencyCertificate certify(String hardwareTier, long validUntil) {
        return new TsLatencyCertificate(
                hardwareTier,
                totalP99Ns(),
                plannerOverheadNs,
                validUntil,
                stages);
    }

    // u64 saturating_add: if the unsigned sum overflows, cap at the unsigned
    // max (which is -1L as a signed long).
    static long saturatingAddUnsigned(long a, long b) {
        long sum = a + b;
        // unsigned overflow happened iff the unsigned sum is below either addend.
        if (Long.compareUnsigned(sum, a) < 0) {
            return -1L;
        }
        return sum;
    }
}
