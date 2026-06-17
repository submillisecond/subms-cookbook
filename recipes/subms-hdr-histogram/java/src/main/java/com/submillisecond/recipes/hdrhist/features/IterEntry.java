package com.submillisecond.recipes.hdrhist.features;

/** One step of histogram iteration. */
public final class IterEntry {
    public final long valueLo;
    public final long valueHi;
    public final long count;
    public final long cumulative;

    public IterEntry(long valueLo, long valueHi, long count, long cumulative) {
        this.valueLo = valueLo;
        this.valueHi = valueHi;
        this.count = count;
        this.cumulative = cumulative;
    }
}
