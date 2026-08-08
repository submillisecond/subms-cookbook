package com.submillisecond.recipes.otel;

import com.submillisecond.recipes.otel.drift.ReferenceDivergenceRecorder;
import com.submillisecond.recipes.otel.exemplars.ExemplarReservoir;
import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;
import com.submillisecond.perf.SubMsStageKind;
import io.opentelemetry.api.common.Attributes;
import io.opentelemetry.api.metrics.DoubleHistogram;
import io.opentelemetry.api.metrics.LongCounter;
import io.opentelemetry.api.metrics.Meter;
import io.opentelemetry.api.metrics.ObservableLongGauge;

import java.util.Objects;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Asynchronous {@link SubMsObserver} for hot-path callers that can't pay a synchronous
 * {@link DoubleHistogram#record} on every op. {@link #onRecord(SubMsObservationCtx, long)} enqueues a tiny
 * {@code Sample} record into a bounded {@link ArrayBlockingQueue} (default capacity 65536); a daemon background thread
 * drains the queue at a configurable interval (default 100 ms) and emits to OpenTelemetry as ns / 1e9 seconds.
 *
 * <p>Always-on counter + gauge surface:
 * <ul>
 *   <li>{@value #OPS_TOTAL_COUNTER_NAME} - +1 per {@link #onRecord(SubMsObservationCtx, long)}.</li>
 *   <li>{@value #IN_FLIGHT_GAUGE_NAME} - observable gauge reporting current queue depth.</li>
 *   <li>{@value #DROPPED_TOTAL_COUNTER_NAME} - +1 per drop under back-pressure.</li>
 *   <li>{@value #EXEMPLARS_KEPT_COUNTER_NAME} - +1 per exemplar retained.</li>
 * </ul>
 *
 * <p><b>Back-pressure policy:</b> drop-newest. If the queue is full, the producer's {@link #onRecord} drops the new
 * sample on the floor and increments the {@code subms.otel.dropped_total} counter so dashboards can flag it.
 *
 * <p><b>Lifecycle:</b> the drain thread is a daemon and stops automatically at JVM shutdown. For deterministic tests
 * call {@link #close()} or {@link #drainNow()}.
 */
public final class OtelObserverAsync implements SubMsObserver, AutoCloseable {

    public static final int DEFAULT_CAPACITY = 65536;
    public static final long DEFAULT_DRAIN_INTERVAL_MS = 100L;

    public static final String DROPPED_TOTAL_COUNTER_NAME = "subms.otel.dropped_total";

    /** Backwards-compatible alias retained so existing dashboards keep matching. */
    public static final String DROPPED_COUNTER_NAME = DROPPED_TOTAL_COUNTER_NAME;

    public static final String OPS_TOTAL_COUNTER_NAME = OtelObserver.OPS_TOTAL_COUNTER_NAME;
    public static final String EXEMPLARS_KEPT_COUNTER_NAME = OtelObserver.EXEMPLARS_KEPT_COUNTER_NAME;
    public static final String IN_FLIGHT_GAUGE_NAME = "subms.bench.in_flight";

    private final Meter meter;
    private final ArrayBlockingQueue<Sample> queue;
    private final long drainIntervalMs;
    private final Thread drainer;
    private final LongCounter dropped;
    private final LongCounter opsTotal;
    private final LongCounter exemplarsKept;
    private final ReferenceDivergenceRecorder drift;
    private final ObservableLongGauge inFlight;
    private final AtomicLong droppedCount = new AtomicLong();
    private volatile DoubleHistogram histogram;
    private final AtomicReference<SubMsBenchSummary> latestSummary = new AtomicReference<>();
    private volatile boolean stopped;
    private volatile ExemplarReservoir exemplars;

    public OtelObserverAsync(Meter meter) {
        this(meter, DEFAULT_CAPACITY, DEFAULT_DRAIN_INTERVAL_MS, true);
    }

    public OtelObserverAsync(Meter meter, int capacity, long drainIntervalMs) {
        this(meter, capacity, drainIntervalMs, true);
    }

    public OtelObserverAsync(Meter meter, int capacity, long drainIntervalMs, boolean startDrainer) {
        if (capacity <= 0) throw new IllegalArgumentException("capacity must be > 0");
        if (drainIntervalMs <= 0) throw new IllegalArgumentException("drainIntervalMs must be > 0");
        this.meter = Objects.requireNonNull(meter, "meter");
        this.queue = new ArrayBlockingQueue<>(capacity);
        this.drainIntervalMs = drainIntervalMs;
        this.dropped = meter.counterBuilder(DROPPED_TOTAL_COUNTER_NAME)
                .setDescription("Samples dropped under back-pressure by OtelObserverAsync")
                .setUnit("1")
                .build();
        this.opsTotal = meter.counterBuilder(OPS_TOTAL_COUNTER_NAME)
                .setDescription("Per-stage operation counter; one increment per onRecord call")
                .setUnit("1")
                .build();
        this.exemplarsKept = meter.counterBuilder(EXEMPLARS_KEPT_COUNTER_NAME)
                .setDescription("Count of samples retained by an attached exemplar reservoir")
                .setUnit("1")
                .build();
        this.drift = new ReferenceDivergenceRecorder(meter);
        this.inFlight = meter.gaugeBuilder(IN_FLIGHT_GAUGE_NAME)
                .ofLongs()
                .setDescription("Pending samples in the async observer's channel.")
                .buildWithCallback(obs -> obs.record(queue.size()));
        if (startDrainer) {
            this.drainer = new Thread(this::drainLoop, "subms-otel-drain");
            this.drainer.setDaemon(true);
            this.drainer.start();
        } else {
            this.drainer = null;
        }
    }

    /** Attach an exemplar reservoir. See {@link OtelObserver#withExemplarReservoir(ExemplarReservoir)}. */
    public OtelObserverAsync withExemplarReservoir(ExemplarReservoir reservoir) {
        this.exemplars = reservoir;
        return this;
    }

    /** Reference to the attached reservoir, or {@code null}. */
    public ExemplarReservoir exemplarReservoir() {
        return exemplars;
    }

    /**
     * Record a reference-impl divergence.
     */
    public void recordReferenceDivergence(
            String stage,
            SubMsStageKind kind,
            String referenceKind,
            String expected,
            String observed) {
        drift.record(stage, kind, referenceKind, expected, observed);
    }

    @Override
    public void onRecord(SubMsObservationCtx ctx, long ns) {
        Sample s = new Sample(ctx.workload(), ctx.lang(), ctx.stage(), ctx.stageKind(), ns);
        Attributes attrs = SubMsOtel.ctxAttributes(ctx.workload(), ctx.lang(), ctx.stage(), ctx.stageKind());
        opsTotal.add(1L, attrs);
        if (!queue.offer(s)) {
            droppedCount.incrementAndGet();
            dropped.add(1L);
        }
        ExemplarReservoir res = exemplars;
        if (res != null && res.offer(ctx, ns)) {
            exemplarsKept.add(1L, attrs);
        }
    }

    @Override
    public void onSummarize(SubMsBenchSummary summary) {
        drainNow();
        latestSummary.set(summary);
        SubMsOtel.exportSummary(summary, meter);
        ExemplarReservoir res = exemplars;
        if (res != null) {
            res.publish(meter);
        }
    }

    public void drainNow() {
        SubMsBenchSummary snap = latestSummary.get();
        Sample s;
        while ((s = queue.poll()) != null) {
            emit(s, snap);
        }
    }

    public long droppedCount() {
        return droppedCount.get();
    }

    public int queueDepth() {
        return queue.size();
    }

    @Override
    public void close() {
        if (stopped) return;
        stopped = true;
        if (drainer != null) {
            drainer.interrupt();
            try {
                drainer.join(1000);
            } catch (InterruptedException ignored) {
                Thread.currentThread().interrupt();
            }
        }
        drainNow();
        try {
            inFlight.close();
        } catch (Exception ignored) {
            // OTEL gauge close is best-effort; nothing actionable on failure
        }
    }

    private void drainLoop() {
        while (!stopped) {
            try {
                Sample first = queue.poll(drainIntervalMs, TimeUnit.MILLISECONDS);
                if (first == null) continue;
                SubMsBenchSummary snap = latestSummary.get();
                emit(first, snap);
                Sample s;
                while ((s = queue.poll()) != null) {
                    emit(s, snap);
                }
            } catch (InterruptedException e) {
                if (stopped) return;
                Thread.currentThread().interrupt();
            }
        }
    }

    private void emit(Sample s, SubMsBenchSummary snap) {
        DoubleHistogram h = histogram();
        Attributes attrs = (snap == null)
                ? SubMsOtel.ctxAttributes(s.workload, s.lang, s.stage, s.kind)
                : SubMsOtel.summaryAttributes(snap, s.stage, s.kind);
        h.record(s.ns / 1e9, attrs);
    }

    private DoubleHistogram histogram() {
        DoubleHistogram h = histogram;
        if (h == null) {
            synchronized (this) {
                h = histogram;
                if (h == null) {
                    h = SubMsOtel.histogramFor(meter);
                    histogram = h;
                }
            }
        }
        return h;
    }

    private record Sample(String workload, String lang, String stage, SubMsStageKind kind, long ns) {}
}
