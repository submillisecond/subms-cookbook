package com.submillisecond.recipes.gorillablock;

/**
 * Summary of a block's contents: point count, timestamp span, and value
 * extremes. Mirrors the Rust {@code TsBlockStats}. On an empty block the value
 * extremes report {@code 0.0}.
 */
public record TsBlockStats(int count, long tsMin, long tsMax, double valueMin, double valueMax) {
}
