package com.submillisecond.recipes.tscardinality;

/**
 * Identity of one ingested point for dedup purposes: which series and the
 * caller-assigned monotonic sequence within it. A replayed
 * {@code (seriesId, sequence)} is the same logical write.
 */
public record TsIngestKey(long seriesId, long sequence) {
    public static TsIngestKey of(long seriesId, long sequence) {
        return new TsIngestKey(seriesId, sequence);
    }
}
