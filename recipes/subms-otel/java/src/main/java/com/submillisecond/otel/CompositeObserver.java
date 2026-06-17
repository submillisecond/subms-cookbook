package com.submillisecond.otel;

import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Objects;

/**
 * Fan out {@link SubMsObserver} callbacks to several inner observers.
 *
 * <p>Use to wire a stack like {@code [OtelObserver, prometheus, logger]} into one harness slot - the harness only
 * accepts one observer, but a {@code CompositeObserver} lets a consumer publish to N sinks at once.
 *
 * <p>Inner observers are invoked in registration order on the harness's recorder thread. Exceptions thrown by one
 * observer are NOT swallowed - they will propagate to the harness; wrap each inner observer in a try-catch if a noisy
 * sink should not break the run.
 */
public final class CompositeObserver implements SubMsObserver {

    private final List<SubMsObserver> observers;

    public CompositeObserver(SubMsObserver... observers) {
        this(Arrays.asList(Objects.requireNonNull(observers, "observers")));
    }

    public CompositeObserver(List<SubMsObserver> observers) {
        Objects.requireNonNull(observers, "observers");
        this.observers = new ArrayList<>(observers);
        for (SubMsObserver o : this.observers) Objects.requireNonNull(o, "observer entry");
    }

    /** Unmodifiable snapshot of the wrapped observers, in registration order. */
    public List<SubMsObserver> observers() {
        return List.copyOf(observers);
    }

    @Override
    public void onRecord(SubMsObservationCtx ctx, long ns) {
        for (SubMsObserver o : observers) o.onRecord(ctx, ns);
    }

    @Override
    public void onSummarize(SubMsBenchSummary summary) {
        for (SubMsObserver o : observers) o.onSummarize(summary);
    }
}
