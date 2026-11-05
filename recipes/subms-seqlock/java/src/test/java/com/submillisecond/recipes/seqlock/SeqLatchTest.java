package com.submillisecond.recipes.seqlock;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SeqLatchTest {

    @Test
    void initialValueAndVersion() {
        SeqLatch<long[]> l = SeqLatch.create(SeqLockImage.longs(2), new long[] {1, 2});
        SeqLatch<long[]>.Writer w = l.writer();
        SeqLatch<long[]>.Reader r = l.reader();
        assertArrayEquals(new long[] {1, 2}, r.read());
        assertEquals(0, r.version());
        assertEquals(0, w.version());
        assertEquals(0, l.version());
        assertArrayEquals(new long[] {1, 2}, w.get());
        assertThrows(IllegalStateException.class, l::writer);
    }

    @Test
    void updateAndSetPublishToBothCopies() {
        SeqLatch<Long> l = SeqLatch.create(SeqLockImage.LONG, 0L);
        SeqLatch<Long>.Writer w = l.writer();
        SeqLatch<Long>.Reader r = l.reader();
        w.update(v -> 5L);
        SeqLock.Versioned<Long> v = r.readVersioned();
        assertEquals(5L, v.value());
        assertEquals(1, v.version());
        w.set(6L);
        assertEquals(6L, r.read());
        assertEquals(2, r.lastVersion());
        assertEquals(6L, w.get());
        assertEquals(6L, w.reader().read());
    }

    @Test
    void boundedReads() {
        SeqLatch<Long> l = SeqLatch.create(SeqLockImage.LONG, 1L);
        l.writer().set(2L);
        SeqLatch<Long>.Reader r = l.reader();
        assertEquals(2L, r.tryRead(1));
        assertEquals(2L, r.tryReadOnce());
        assertNull(r.tryRead(0));
    }

    @Test
    void emptyImageIsRefused() {
        assertThrows(IllegalArgumentException.class, () -> SeqLatch.create(SeqLockImage.longs(0), new long[0]));
    }

    @Test
    void stressReadersNeverSeeATornImageOrAMismatchedVersionPair() throws InterruptedException {
        long updates = 300_000;
        SeqLatch<long[]> l = SeqLatch.create(SeqLockImage.longs(6), new long[6]);
        SeqLatch<long[]>.Writer w = l.writer();
        AtomicLong torn = new AtomicLong();
        AtomicLong reads = new AtomicLong();
        List<Thread> readers = new ArrayList<>();
        for (int t = 0; t < 3; t++) {
            Thread th = new Thread(() -> {
                SeqLatch<long[]>.Reader r = l.reader();
                long last = 0;
                while (last < updates) {
                    SeqLock.Versioned<long[]> v = r.readVersioned();
                    long[] img = v.value();
                    for (long x : img) {
                        if (x != img[0]) {
                            torn.incrementAndGet();
                        }
                    }
                    if (v.version() != img[0] || img[0] < last) {
                        torn.incrementAndGet();
                    }
                    last = img[0];
                    reads.incrementAndGet();
                }
            });
            th.start();
            readers.add(th);
        }
        long[] img = new long[6];
        for (long i = 1; i <= updates; i++) {
            Arrays.fill(img, i);
            w.set(img);
        }
        for (Thread th : readers) {
            th.join();
        }
        assertEquals(0, torn.get());
        assertTrue(reads.get() >= 3);
        assertEquals(updates, w.version());
        assertEquals(updates, l.reader().read()[5]);
    }
}
