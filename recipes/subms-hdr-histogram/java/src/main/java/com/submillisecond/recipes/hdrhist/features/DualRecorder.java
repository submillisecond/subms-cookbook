package com.submillisecond.recipes.hdrhist.features;

import java.util.concurrent.atomic.AtomicInteger;

/**
 * Recorder pattern: two histograms alternated for lock-free
 * producer / occasional consumer.
 *
 * <p>Producers call {@link #record(long)} on the recorder, which
 * forwards to the currently-active concurrent histogram. The consumer
 * calls {@link #getIntervalHistogram()} to atomically rotate sides
 * and drain the newly-inactive one. The producer hot path never
 * blocks - always one valid histogram, accessible without locks.
 *
 * <p>Pattern taken from HdrHistogram-Java's {@code Recorder}. Useful
 * for interval reporting loops: every N seconds the consumer grabs
 * an interval snapshot without disturbing producers.
 */
public final class DualRecorder {

    private final ConcurrentHdrHistogram[] histograms;
    private final AtomicInteger active = new AtomicInteger(0);

    public DualRecorder(int significantDigits) {
        this(significantDigits, 32);
    }

    public DualRecorder(int significantDigits, int majors) {
        this.histograms = new ConcurrentHdrHistogram[] {
            new ConcurrentHdrHistogram(significantDigits, majors),
            new ConcurrentHdrHistogram(significantDigits, majors),
        };
    }

    /** Record into the currently-active histogram. Lock-free. */
    public void record(long value) {
        histograms[active.get()].record(value);
    }

    /**
     * Atomically rotate the active side and drain the newly-inactive
     * side. Producers that race the rotation may land their write on
     * EITHER side - both valid live targets at that moment. The
     * returned snapshot reflects all records that landed on the
     * outgoing side before rotation completed.
     *
     * <p>Only one consumer thread should call this at a time.
     */
    public Snapshot getIntervalHistogram() {
        int prev = active.get();
        int next = 1 - prev;
        active.set(next);
        return histograms[prev].drainSnapshot();
    }

    /** Currently-active index. Exposed for tests / observability. */
    public int activeIndex() { return active.get(); }
}
