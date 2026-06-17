package com.submillisecond.recipes.lsm.features;

import java.util.Optional;

/**
 * The trait the read path calls. Implementations decide replacement policy,
 * concurrency, and whether to admit insertions.
 *
 * <p>Production caches (TinyLFU, segmented LRU) live in the sibling recipe
 * {@code subms-block-cache}; this is the wiring contract, not the policy
 * ceiling. {@link LruBlockCache} is the reference impl shipped here.
 */
public interface BlockCache {

    /** Returns the cached payload if present. */
    Optional<byte[]> get(BlockKey key);

    /** Insert a block. May evict to honour capacity bounds. */
    void put(BlockKey key, byte[] block);

    /** Current number of cached entries. */
    int size();

    /** True if no entries are cached. */
    default boolean isEmpty() {
        return size() == 0;
    }

    /** Drop every entry. Used by tests + manifest swaps. */
    void clear();
}
