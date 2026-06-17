package com.submillisecond.otel.autoconfig;

import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;

import java.util.concurrent.atomic.AtomicLong;

/** Test-only observer used to verify ServiceLoader discovery. */
public final class FakeServiceLoadedObserver implements SubMsObserver {

    public final AtomicLong records = new AtomicLong();
    public final AtomicLong summaries = new AtomicLong();

    public FakeServiceLoadedObserver() {}

    @Override
    public void onRecord(SubMsObservationCtx ctx, long ns) {
        records.incrementAndGet();
    }

    @Override
    public void onSummarize(SubMsBenchSummary summary) {
        summaries.incrementAndGet();
    }
}
