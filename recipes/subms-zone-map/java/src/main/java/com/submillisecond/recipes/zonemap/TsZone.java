package com.submillisecond.recipes.zonemap;

/**
 * One summary tuple per block: its id, timestamp span, value extremes, and
 * point count. Mirrors the Rust {@code TsZone}.
 */
public record TsZone(
        long blockId,
        long tsMin,
        long tsMax,
        double valueMin,
        double valueMax,
        int count) {
}
