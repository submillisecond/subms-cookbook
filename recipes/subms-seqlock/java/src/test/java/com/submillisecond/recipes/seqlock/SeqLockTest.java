package com.submillisecond.recipes.seqlock;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class SeqLockTest {

    record Quote(long bid, long ask, double mid) {}

    static final SeqLockImage<Quote> QUOTE = new SeqLockImage<>() {
        @Override
        public int words() {
            return 3;
        }

        @Override
        public void store(Quote v, WordSink put) {
            put.put(0, v.bid());
            put.put(1, v.ask());
            put.put(2, Double.doubleToRawLongBits(v.mid()));
        }

        @Override
        public Quote load(WordSource get) {
            return new Quote(get.get(0), get.get(1), Double.longBitsToDouble(get.get(2)));
        }
    };

    @Test
    void initialValueReadsBackAtVersionZero() {
        SeqLock<Long> lock = SeqLock.create(SeqLockImage.LONG, 42L);
        SeqLock<Long>.Writer w = lock.writer();
        SeqLock<Long>.Reader r = lock.reader();
        assertEquals(42L, r.read());
        assertEquals(0, r.version());
        assertEquals(0, w.version());
        assertEquals(42L, w.get());
        assertFalse(r.isWriting());
        assertFalse(lock.isWriting());
    }

    @Test
    void updateEditsTheShadowAndPublishes() {
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(3), new long[] {1, 2, 3});
        SeqLock<long[]>.Writer w = lock.writer();
        SeqLock<long[]>.Reader r = lock.reader();
        w.update(v -> new long[] {v[0], 20, v[2]});
        assertArrayEquals(new long[] {1, 20, 3}, r.read());
        w.update(v -> new long[] {v[0] + 100, v[1], v[2]});
        assertArrayEquals(new long[] {101, 20, 3}, w.get());
        SeqLock.Versioned<long[]> v = r.readVersioned();
        assertArrayEquals(new long[] {101, 20, 3}, v.value());
        assertEquals(2, v.version());
        assertEquals(2, r.lastVersion());
    }

    @Test
    void setReplacesAndBumpsTheVersion() {
        SeqLock<Long> lock = SeqLock.create(SeqLockImage.LONG, -5L);
        SeqLock<Long>.Writer w = lock.writer();
        w.set(7L);
        w.set(-9L);
        assertEquals(-9L, lock.reader().read());
        assertEquals(2, lock.version());
    }

    @Test
    void providedImagesRoundTrip() {
        SeqLock<Double> d = SeqLock.create(SeqLockImage.DOUBLE, Double.NaN);
        SeqLock<Double>.Writer dw = d.writer();
        assertTrue(Double.isNaN(d.reader().read()));
        dw.set(-0.0);
        assertEquals(Double.doubleToRawLongBits(-0.0), Double.doubleToRawLongBits(d.reader().read()));
        SeqLock<long[]> a = SeqLock.create(SeqLockImage.longs(2), new long[] {-1, Long.MIN_VALUE});
        a.writer().set(new long[] {Long.MAX_VALUE, 0});
        assertArrayEquals(new long[] {Long.MAX_VALUE, 0}, a.reader().read());
    }

    @Test
    void customRecordImageRoundTrips() {
        Quote q = new Quote(99_995, 100_005, 100_000.0);
        SeqLock<Quote> lock = SeqLock.create(QUOTE, q);
        SeqLock<Quote>.Writer w = lock.writer();
        assertEquals(q, lock.reader().read());
        w.update(v -> new Quote(-1, v.ask(), 0.5));
        assertEquals(new Quote(-1, 100_005, 0.5), lock.reader().read());
    }

    @Test
    void oneWriterManyReaders() {
        SeqLock<Long> lock = SeqLock.create(SeqLockImage.LONG, 0L);
        SeqLock<Long>.Writer w = lock.writer();
        assertThrows(IllegalStateException.class, lock::writer);
        SeqLock<Long>.Reader a = lock.reader();
        SeqLock<Long>.Reader b = w.reader();
        w.set(9L);
        assertEquals(9L, a.read());
        assertEquals(9L, b.read());
        assertEquals(1, b.version());
    }

    @Test
    void tryReadOnAnIdleLockAndTryReadOnce() {
        SeqLock<Long> lock = SeqLock.create(SeqLockImage.LONG, 3L);
        SeqLock<Long>.Writer w = lock.writer();
        SeqLock<Long>.Reader r = lock.reader();
        assertEquals(3L, r.tryRead(1));
        w.set(4L);
        assertEquals(4L, r.tryReadOnce());
        assertEquals(1, r.lastVersion());
        assertNull(r.tryRead(0));
    }

    @Test
    void emptyImageIsRefused() {
        assertThrows(IllegalArgumentException.class, () -> SeqLock.create(SeqLockImage.longs(0), new long[0]));
    }

    @Test
    void wideImageRoundTrips() {
        long[] init = new long[512];
        java.util.Arrays.fill(init, 7);
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(512), init);
        SeqLock<long[]>.Writer w = lock.writer();
        w.update(v -> {
            long[] n = v.clone();
            n[511] = 1;
            return n;
        });
        long[] got = lock.reader().read();
        assertEquals(7, got[0]);
        assertEquals(1, got[511]);
    }

    @Test
    void stressFourReadersNeverObserveATornImage() throws InterruptedException {
        long updates = 300_000;
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(8), new long[8]);
        SeqLock<long[]>.Writer w = lock.writer();
        AtomicLong torn = new AtomicLong();
        AtomicLong reads = new AtomicLong();
        List<Thread> readers = new ArrayList<>();
        for (int t = 0; t < 4; t++) {
            Thread th = new Thread(() -> {
                SeqLock<long[]>.Reader r = lock.reader();
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
        long[] img = new long[8];
        for (long i = 1; i <= updates; i++) {
            java.util.Arrays.fill(img, i);
            w.set(img);
        }
        for (Thread th : readers) {
            th.join();
        }
        assertEquals(0, torn.get());
        assertTrue(reads.get() >= 4);
        assertEquals(updates, w.version());
        long[] fin = lock.reader().read();
        assertNotNull(fin);
        assertEquals(updates, fin[7]);
    }
}
