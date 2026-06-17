package com.submillisecond.recipes.tscardinality;

/**
 * A series-count cap. Tracks its own admitted count and decides admit vs
 * reject. Pure counter arithmetic - the guard never sees the series itself.
 */
public final class TsCardinalityGuard {

    private final int max;
    private final TsOverflowPolicy policy;
    private int count;

    public TsCardinalityGuard(int maxSeries, TsOverflowPolicy policy) {
        this.max = maxSeries;
        this.policy = policy;
        this.count = 0;
    }

    /**
     * Try to admit one series. Increments the count when admitted. Under
     * {@link TsOverflowPolicy#REJECT} a full guard throws
     * {@link TsCardinalityException} and leaves the count unchanged; under
     * {@link TsOverflowPolicy#ALLOW} it always admits and the count is allowed
     * to climb past {@code max}.
     */
    public void admit() {
        if (count >= max && policy == TsOverflowPolicy.REJECT) {
            throw TsCardinalityException.cardinalityCap(max);
        }
        count++;
    }

    /** Release one admitted slot. Saturates at zero. */
    public void release() {
        if (count > 0) count--;
    }

    public int count() {
        return count;
    }

    public int max() {
        return max;
    }

    /** Free slots before the cap binds; zero once the count reaches {@code max}. */
    public int remaining() {
        return Math.max(0, max - count);
    }

    /** How far the count has climbed past {@code max}; always zero under REJECT. */
    public int overCount() {
        return Math.max(0, count - max);
    }

    /** True when the next REJECT admit would fail (or ALLOW would over-admit). */
    public boolean wouldExceed() {
        return count >= max;
    }
}
