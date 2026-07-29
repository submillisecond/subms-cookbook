package com.submillisecond.recipes.art.features;

/** Snapshot of {@link MeasuredArt} counters at a moment in time. */
public final class ArtMetrics {
    public final long lookups;
    public final long insertions;
    public final long deletions;
    public final int lastDepth;
    public final int node4;
    public final int node16;
    public final int node48;
    public final int node256;
    public final int entries;

    public ArtMetrics(long lookups, long insertions, long deletions, int lastDepth,
                      int node4, int node16, int node48, int node256, int entries) {
        this.lookups = lookups;
        this.insertions = insertions;
        this.deletions = deletions;
        this.lastDepth = lastDepth;
        this.node4 = node4;
        this.node16 = node16;
        this.node48 = node48;
        this.node256 = node256;
        this.entries = entries;
    }
}
