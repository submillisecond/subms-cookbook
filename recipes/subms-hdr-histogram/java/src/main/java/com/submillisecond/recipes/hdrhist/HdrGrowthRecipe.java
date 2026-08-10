package com.submillisecond.recipes.hdrhist;

import java.util.Map;

import com.submillisecond.perf.SubMsGrowth;
import com.submillisecond.perf.SubMsGrowthRecipe;

/**
 * Records an unbounded stream of values and reports the counter array's real
 * size each round. The array is sized by the largest value seen, never by how
 * many values were recorded, so the curve is flat in the sample count.
 *
 * <p>Mirror of the Rust {@code growth} module, driven by {@link GrowthMain} from
 * the same stdin key=value config, so the two curves are directly comparable.
 */
public final class HdrGrowthRecipe implements SubMsGrowthRecipe {

    /**
     * Exclusive top of the recorded value range. It fixes the counter array's
     * maximum size, so it also fixes the bound the verdict is checked against.
     */
    private static final long VALUE_CEILING = 1_000_000_000L;

    private final HdrHistogram hist;
    private final int rounds;
    private final int recordsPerRound;
    private final long boundBytes;
    private long lcg = 0x1234_5678L;

    public HdrGrowthRecipe(int significantDigits, int rounds, int recordsPerRound) {
        this.hist = new HdrHistogram(significantDigits);
        this.rounds = rounds;
        this.recordsPerRound = recordsPerRound;
        this.boundBytes = ceilingFootprintBytes(significantDigits);
    }

    /**
     * The array size once the top of the value range has been recorded, measured
     * on a throwaway histogram rather than modelled. A second footprint model in
     * the harness is what published a figure 2.4x under the real allocation.
     */
    private static long ceilingFootprintBytes(int significantDigits) {
        HdrHistogram probe = new HdrHistogram(significantDigits);
        probe.record(VALUE_CEILING - 1);
        return probe.footprintBytes();
    }

    @Override public String name() {
        return "subms-hdr-histogram";
    }

    @Override public String opName() {
        return "record";
    }

    @Override public int rounds() {
        return rounds;
    }

    @Override public int opsPerRound() {
        return recordsPerRound;
    }

    @Override public void op(int round, int i) {
        // A spread of values across the whole range, so every major bucket is
        // touched - still O(1) memory once the array covers the range.
        lcg = lcg * 6364136223846793005L + 1;
        long v = (lcg >>> 33) % VALUE_CEILING;
        hist.record(Math.max(v, 1));
    }

    @Override public long memoryBytes() {
        return hist.footprintBytes();
    }

    @Override public long liveBytes() {
        return hist.footprintBytes();
    }

    @Override public Map<String, Long> structures() {
        return Map.of("records", hist.count());
    }

    @Override public SubMsGrowth.GrowthClass expectedClass() {
        return SubMsGrowth.GrowthClass.BOUNDED;
    }

    @Override public double expectedBound() {
        return (double) boundBytes * 1.01;
    }

    @Override public boolean compact() {
        return true;
    }
}
