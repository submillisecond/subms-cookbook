package com.submillisecond.recipes.art.features;

/** Snapshot of {@link MeasuredArt} counters at a moment in time. */
public final class ArtMetrics {
    public final long lookups;
    public final long insertions;
    public final long deletions;
    public final int lastDepth;
    public final int smallNodes;
    public final int fullNodes;
    public final int entries;

    public ArtMetrics(long lookups, long insertions, long deletions,
                      int lastDepth, int smallNodes, int fullNodes, int entries) {
        this.lookups = lookups;
        this.insertions = insertions;
        this.deletions = deletions;
        this.lastDepth = lastDepth;
        this.smallNodes = smallNodes;
        this.fullNodes = fullNodes;
        this.entries = entries;
    }
}
