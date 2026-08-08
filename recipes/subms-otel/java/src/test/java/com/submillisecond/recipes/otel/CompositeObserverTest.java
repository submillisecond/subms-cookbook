package com.submillisecond.recipes.otel;

import com.submillisecond.perf.SubMsBenchSummary;
import com.submillisecond.perf.SubMsObservationCtx;
import com.submillisecond.perf.SubMsObserver;
import com.submillisecond.perf.SubMsStageKind;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class CompositeObserverTest {

    static final class Recording implements SubMsObserver {
        final List<Long> records = new ArrayList<>();
        final AtomicInteger summaries = new AtomicInteger();

        @Override
        public void onRecord(SubMsObservationCtx ctx, long ns) {
            records.add(ns);
        }

        @Override
        public void onSummarize(SubMsBenchSummary summary) {
            summaries.incrementAndGet();
        }
    }

    @Test
    void fansOutToBothObservers() {
        Recording a = new Recording();
        Recording b = new Recording();
        CompositeObserver c = new CompositeObserver(a, b);

        SubMsObservationCtx ctx = new SubMsObservationCtx("wl", "java", "put", SubMsStageKind.HOT_PATH);
        for (int i = 0; i < 10; i++) c.onRecord(ctx, i);

        SubMsBenchSummary summary = new SubMsBenchSummary(
                "wl", "java", "ts", null, null, Map.of(), Map.of(), List.of());
        c.onSummarize(summary);

        assertEquals(10, a.records.size());
        assertEquals(10, b.records.size());
        assertEquals(a.records, b.records);
        assertEquals(1, a.summaries.get());
        assertEquals(1, b.summaries.get());
    }

    @Test
    void preservesRegistrationOrder() {
        List<String> order = new ArrayList<>();
        SubMsObserver first = new SubMsObserver() {
            @Override public void onRecord(SubMsObservationCtx ctx, long ns) { order.add("first"); }
        };
        SubMsObserver second = new SubMsObserver() {
            @Override public void onRecord(SubMsObservationCtx ctx, long ns) { order.add("second"); }
        };
        CompositeObserver c = new CompositeObserver(first, second);
        c.onRecord(new SubMsObservationCtx("w", "j", "s", SubMsStageKind.HOT_PATH), 1L);
        assertEquals(List.of("first", "second"), order);
    }

    @Test
    void observersAccessorIsUnmodifiable() {
        Recording a = new Recording();
        CompositeObserver c = new CompositeObserver(a);
        List<SubMsObserver> snap = c.observers();
        assertEquals(1, snap.size());
        assertSame(a, snap.get(0));
        assertThrows(UnsupportedOperationException.class, () -> snap.add(new Recording()));
    }

    @Test
    void nullObserverRejected() {
        Recording a = new Recording();
        assertThrows(NullPointerException.class, () -> new CompositeObserver(a, null));
        assertThrows(NullPointerException.class, () -> new CompositeObserver((SubMsObserver[]) null));
        assertThrows(NullPointerException.class, () -> new CompositeObserver((List<SubMsObserver>) null));
    }

    @Test
    void emptyCompositeIsNoOp() {
        CompositeObserver c = new CompositeObserver();
        c.onRecord(new SubMsObservationCtx("w", "j", "s", SubMsStageKind.UNSPECIFIED), 1L);
        c.onSummarize(new SubMsBenchSummary("w", "j", "ts", null, null, Map.of(), Map.of(), List.of()));
        assertTrue(c.observers().isEmpty());
    }
}
