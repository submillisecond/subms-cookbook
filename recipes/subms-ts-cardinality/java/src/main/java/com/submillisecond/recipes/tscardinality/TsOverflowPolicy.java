package com.submillisecond.recipes.tscardinality;

/** What a guard does once its cap is reached. */
public enum TsOverflowPolicy {
    /** Refuse the admission: {@code admit} throws a {@link TsCardinalityException}. */
    REJECT,
    /**
     * Admit anyway and keep counting. The cap becomes a soft watermark you can
     * read back via {@link TsCardinalityGuard#overCount()}.
     */
    ALLOW
}
