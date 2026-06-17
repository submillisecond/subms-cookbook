package com.submillisecond.recipes.mpsc.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MetricsMpscQueueTest {

    @Test
    void initialSnapshotIsAllZero() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        MetricsMpscQueue.Snapshot s = q.snapshot();
        assertEquals(0, s.enqueueOk);
        assertEquals(0, s.enqueueFail);
        assertEquals(0, s.dequeueOk);
        assertEquals(0, s.dequeueFail);
        assertEquals(0, s.batchItems);
        assertEquals(0, s.casRetries);
    }

    @Test
    void pushIncrementsEnqueueOk() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        for (int i = 0; i < 5; i++) q.push(i);
        MetricsMpscQueue.Snapshot s = q.snapshot();
        assertEquals(5, s.enqueueOk);
        assertEquals(0, s.enqueueFail);
    }

    @Test
    void tryPollTracksOkAndFail() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        q.push(1);
        q.push(2);
        // Drain.
        while (q.tryPoll() == null) Thread.onSpinWait();
        while (q.tryPoll() == null) Thread.onSpinWait();
        // Now empty:
        assertNull(q.tryPoll());
        MetricsMpscQueue.Snapshot s = q.snapshot();
        assertEquals(2, s.dequeueOk);
        assertTrue(s.dequeueFail >= 1);
    }

    @Test
    void batchRecordsDrainedItems() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        for (int i = 0; i < 7; i++) q.push(i);
        Integer[] buf = new Integer[10];
        int n = q.tryPollBatch(buf);
        MetricsMpscQueue.Snapshot s = q.snapshot();
        assertEquals(7, n);
        assertEquals(7, s.batchItems);
        assertEquals(7, s.dequeueOk);
    }

    @Test
    void recordEnqueueFailBumpsCounter() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        q.push(1);
        q.recordEnqueueFail();
        q.recordEnqueueFail();
        MetricsMpscQueue.Snapshot s = q.snapshot();
        assertEquals(1, s.enqueueOk);
        assertEquals(2, s.enqueueFail);
    }

    @Test
    void recordCasRetriesAccumulates() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        q.recordCasRetries(5);
        q.recordCasRetries(3);
        q.recordCasRetries(0); // no-op
        assertEquals(8, q.snapshot().casRetries);
    }

    @Test
    void resetClearsAllCounters() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        q.push(1);
        q.push(2);
        q.tryPoll();
        q.recordEnqueueFail();
        q.recordCasRetries(7);
        q.reset();
        MetricsMpscQueue.Snapshot s = q.snapshot();
        assertEquals(0, s.enqueueOk);
        assertEquals(0, s.dequeueOk);
        assertEquals(0, s.enqueueFail);
        assertEquals(0, s.casRetries);
    }

    @Test
    void snapshotToStringIsNonEmpty() {
        MetricsMpscQueue<Integer> q = new MetricsMpscQueue<>();
        q.push(1);
        String s = q.snapshot().toString();
        assertNotNull(s);
        assertTrue(s.contains("enqueueOk"));
    }
}
