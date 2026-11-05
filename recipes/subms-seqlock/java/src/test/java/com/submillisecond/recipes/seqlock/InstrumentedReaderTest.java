package com.submillisecond.recipes.seqlock;

import org.junit.jupiter.api.Test;

import java.util.Arrays;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class InstrumentedReaderTest {

    @Test
    void idleReadsCountWithNoRetries() {
        SeqLock<Long> lock = SeqLock.create(SeqLockImage.LONG, 1L);
        SeqLock<Long>.Writer w = lock.writer();
        InstrumentedReader<Long> counted = new InstrumentedReader<>(lock.reader());
        for (long i = 0; i < 100; i++) {
            w.set(i);
            assertEquals(i, counted.read());
        }
        ReadStats.Snapshot s = counted.snapshot();
        assertEquals(100, s.reads());
        assertEquals(0, s.retries());
        assertEquals(0, s.maxRetries());
        assertEquals(0.0, s.retryRate());
        SeqLock.Versioned<Long> v = counted.readVersioned();
        assertEquals(99L, v.value());
        assertEquals(100, v.version());
        assertEquals(99L, counted.tryRead(1));
        assertEquals(102, counted.snapshot().reads());
        assertEquals(100, counted.inner().version());
    }

    @Test
    void sharedStatsAndReset() {
        SeqLock<Long> lock = SeqLock.create(SeqLockImage.LONG, 0L);
        SeqLock<Long>.Writer w = lock.writer();
        ReadStats stats = new ReadStats();
        InstrumentedReader<Long> a = new InstrumentedReader<>(lock.reader(), stats);
        InstrumentedReader<Long> b = new InstrumentedReader<>(lock.reader(), stats);
        w.set(1L);
        a.read();
        b.read();
        b.read();
        assertEquals(3, stats.snapshot().reads());
        assertSame(stats, a.stats());
        stats.reset();
        assertEquals(new ReadStats.Snapshot(0, 0, 0), stats.snapshot());
        assertEquals(0.0, new ReadStats.Snapshot(0, 0, 0).retryRate());
        assertEquals(0.5, new ReadStats.Snapshot(10, 5, 2).retryRate());
    }

    @Test
    void failedBoundedReadsAreRecordedAsRetries() {
        ReadStats stats = new ReadStats();
        stats.failed(5);
        stats.failed(2);
        ReadStats.Snapshot s = stats.snapshot();
        assertEquals(0, s.reads());
        assertEquals(7, s.retries());
        assertEquals(5, s.maxRetries());
        stats.record(3);
        assertEquals(1, stats.snapshot().reads());
        assertEquals(10, stats.snapshot().retries());
    }

    @Test
    void stressCountedReadsUnderALiveWriterStayConsistent() throws InterruptedException {
        long updates = 200_000;
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(4), new long[4]);
        SeqLock<long[]>.Writer w = lock.writer();
        ReadStats stats = new ReadStats();
        long[] torn = {0};
        Thread reader = new Thread(() -> {
            InstrumentedReader<long[]> r = new InstrumentedReader<>(lock.reader(), stats);
            long last = 0;
            while (last < updates) {
                long[] img = r.read();
                for (long x : img) {
                    if (x != img[0]) {
                        torn[0]++;
                    }
                }
                last = img[0];
            }
        });
        reader.start();
        long[] img = new long[4];
        for (long i = 1; i <= updates; i++) {
            Arrays.fill(img, i);
            w.set(img);
        }
        reader.join();
        assertEquals(0, torn[0]);
        ReadStats.Snapshot s = stats.snapshot();
        assertNotNull(s);
        assertTrue(s.reads() >= 1);
        assertTrue(s.retryRate() >= 0.0);
    }
}
