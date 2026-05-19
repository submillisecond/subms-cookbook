package com.submillisecond.recipes.lsm;

/**
 * Read-path bloom-filter behaviour. The filter is always <em>written</em>
 * into every SSTable trailer - this just controls whether reads consult it.
 */
public enum BloomMode {
    /** Check the bloom filter before scanning each SSTable. Default. */
    ON,
    /** Skip the bloom probe. Every SSTable in the walk pays a full scan. */
    OFF
}
