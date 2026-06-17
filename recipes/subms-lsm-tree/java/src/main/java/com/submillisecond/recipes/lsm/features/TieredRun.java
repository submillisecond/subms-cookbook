package com.submillisecond.recipes.lsm.features;

import java.util.List;

/**
 * One run of (key, optional value) entries inside a {@link TieredManifest}.
 * Tombstones are represented as entries with {@code value == null}.
 */
public final class TieredRun {

    public final long id;
    public final long sizeBytes;
    public final List<Entry> entries;

    public TieredRun(long id, List<Entry> entries) {
        this.id = id;
        this.entries = entries;
        long size = 0;
        for (Entry e : entries) {
            size += e.key().length() + (e.value() == null ? 1 : e.value().length);
        }
        this.sizeBytes = size;
    }

    public record Entry(String key, byte[] value) { }
}
