package com.submillisecond.recipes.seqlock;

/** A {@link SeqLock.Reader} that records what every read cost. One per thread, like the
 *  reader it wraps; the counters are shared. */
public final class InstrumentedReader<T> {

    private final SeqLock<T>.Reader reader;
    private final ReadStats stats;

    public InstrumentedReader(SeqLock<T>.Reader reader) {
        this(reader, new ReadStats());
    }

    public InstrumentedReader(SeqLock<T>.Reader reader, ReadStats stats) {
        this.reader = reader;
        this.stats = stats;
    }

    public T read() {
        long attempts = 0;
        for (;;) {
            T v = reader.tryReadOnce();
            if (v != null) {
                stats.record(attempts);
                return v;
            }
            attempts++;
            Thread.onSpinWait();
        }
    }

    public SeqLock.Versioned<T> readVersioned() {
        T v = read();
        return new SeqLock.Versioned<>(v, reader.lastVersion());
    }

    /** At most {@code maxAttempts} tries. A failure is recorded as its attempts with no read
     *  counted. */
    public T tryRead(int maxAttempts) {
        long attempts = 0;
        for (int i = 0; i < maxAttempts; i++) {
            T v = reader.tryReadOnce();
            if (v != null) {
                stats.record(attempts);
                return v;
            }
            attempts++;
            Thread.onSpinWait();
        }
        if (attempts > 0) {
            stats.failed(attempts);
        }
        return null;
    }

    public ReadStats stats() {
        return stats;
    }

    public ReadStats.Snapshot snapshot() {
        return stats.snapshot();
    }

    public SeqLock<T>.Reader inner() {
        return reader;
    }
}
