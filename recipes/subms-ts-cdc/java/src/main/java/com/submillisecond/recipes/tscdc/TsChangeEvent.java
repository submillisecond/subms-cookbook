package com.submillisecond.recipes.tscdc;

/**
 * A single mutation observed on the collection, published in commit order.
 *
 * <p>One implementation per mirrored mutator: {@link Push} carries the appended
 * sample, {@link DeleteAt} / {@link DeleteRange} the key span removed,
 * {@link Deregister} a whole series leaving the registry.
 *
 * @param <T> the series value type
 */
public sealed interface TsChangeEvent<T>
        permits TsChangeEvent.Push, TsChangeEvent.DeleteAt,
                TsChangeEvent.DeleteRange, TsChangeEvent.Deregister {

    long seriesId();

    record Push<T>(long seriesId, long ts, T value) implements TsChangeEvent<T> {}

    record DeleteAt<T>(long seriesId, long ts) implements TsChangeEvent<T> {}

    record DeleteRange<T>(long seriesId, long lo, long hi) implements TsChangeEvent<T> {}

    record Deregister<T>(long seriesId) implements TsChangeEvent<T> {}
}
