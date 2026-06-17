package com.submillisecond.recipes.tsanomaly;

/**
 * A flagged point: its timestamp, value, and the z-score that tripped the
 * threshold (signed - positive above the baseline, negative below).
 */
public record TsAnomaly(long ts, double value, double zscore) {
}
