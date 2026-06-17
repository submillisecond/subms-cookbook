package com.submillisecond.recipes.lsm.features;

/**
 * Cache lookup key: (sstable id, block byte offset within the file).
 *
 * <p>{@code record} gives us value-equality / hashing for free, which the
 * cache relies on as a HashMap key.
 */
public record BlockKey(long sstableId, long blockOffset) { }
