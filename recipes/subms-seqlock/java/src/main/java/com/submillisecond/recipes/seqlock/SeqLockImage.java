package com.submillisecond.recipes.seqlock;

/**
 * How a value is laid out as a fixed run of 64-bit words. The lock streams the words straight
 * through its own storage, so neither path has an intermediate buffer.
 */
public interface SeqLockImage<T> {

    @FunctionalInterface
    interface WordSink {
        void put(int i, long word);
    }

    @FunctionalInterface
    interface WordSource {
        long get(int i);
    }

    int words();

    void store(T value, WordSink put);

    T load(WordSource get);

    /** A {@code long[]} of {@code n} words; {@code load} returns a fresh array. */
    static SeqLockImage<long[]> longs(int n) {
        return new SeqLockImage<>() {
            @Override
            public int words() {
                return n;
            }

            @Override
            public void store(long[] value, WordSink put) {
                for (int i = 0; i < n; i++) {
                    put.put(i, value[i]);
                }
            }

            @Override
            public long[] load(WordSource get) {
                long[] out = new long[n];
                for (int i = 0; i < n; i++) {
                    out[i] = get.get(i);
                }
                return out;
            }
        };
    }

    /** A single {@code long}. */
    SeqLockImage<Long> LONG = new SeqLockImage<>() {
        @Override
        public int words() {
            return 1;
        }

        @Override
        public void store(Long value, WordSink put) {
            put.put(0, value);
        }

        @Override
        public Long load(WordSource get) {
            return get.get(0);
        }
    };

    /** A single {@code double}, carried as its raw bits. */
    SeqLockImage<Double> DOUBLE = new SeqLockImage<>() {
        @Override
        public int words() {
            return 1;
        }

        @Override
        public void store(Double value, WordSink put) {
            put.put(0, Double.doubleToRawLongBits(value));
        }

        @Override
        public Double load(WordSource get) {
            return Double.longBitsToDouble(get.get(0));
        }
    };
}
