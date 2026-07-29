package com.submillisecond.recipes.lsm;

/** When and where a full memtable is turned into an SSTable. */
public enum FlushMode {
    /**
     * The default. A full memtable is handed to a background thread and a fresh
     * memtable is installed immediately, so the triggering write pays only the
     * swap, not the O(memtable) flush. This is what keeps the write tail flat.
     */
    BACKGROUND,
    /**
     * The triggering write flushes inline on the caller's thread. Deterministic
     * and thread-free (single-threaded targets, deterministic replay) at the cost
     * of a periodic write-latency spike.
     */
    SYNC
}
