package com.submillisecond.recipes.ts;

/**
 * Gate for "is this a present (non-null) observation". Value types the
 * library ships implement it; a custom value type implements it once. The
 * default treats every value as present. {@link TsSeries#push} rejects values
 * that report {@code false}.
 */
public interface TsValueKind {
    default boolean tsIsPresent() {
        return true;
    }
}
