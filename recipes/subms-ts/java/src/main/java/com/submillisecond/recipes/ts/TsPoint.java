package com.submillisecond.recipes.ts;

import java.time.Instant;
import java.util.Objects;

/**
 * A single observation: an {@code i64}-nanosecond timestamp paired with a
 * value. The value type is generic - {@code Double} for the scalar fast path,
 * {@link Ohlc} / {@link Curve} / {@link Surface} for compound shapes, or any
 * custom type. {@code TsPoint} is the iteration item and the input to
 * {@code push}; it is not the storage layout (the series stores SoA columns
 * internally).
 */
public record TsPoint<T>(long ts, T value) {

    public static <T> TsPoint<T> of(long ts, T value) {
        return new TsPoint<>(ts, value);
    }

    /** java.time sugar: build a point at an {@link Instant}, nanos resolution. */
    public static <T> TsPoint<T> atInstant(Instant when, T value) {
        long ns = Math.addExact(Math.multiplyExact(when.getEpochSecond(), 1_000_000_000L), when.getNano());
        return new TsPoint<>(ns, value);
    }

    /** This point's timestamp as an {@link Instant} (nanos resolution). */
    public Instant instant() {
        long secs = Math.floorDiv(ts, 1_000_000_000L);
        long nanos = Math.floorMod(ts, 1_000_000_000L);
        return Instant.ofEpochSecond(secs, nanos);
    }

    @Override
    public boolean equals(Object o) {
        if (this == o) return true;
        if (!(o instanceof TsPoint<?> p)) return false;
        return ts == p.ts && Objects.equals(value, p.value);
    }

    @Override
    public int hashCode() {
        return Objects.hash(ts, value);
    }
}
