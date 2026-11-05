package com.submillisecond.recipes.seqlock;

import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class SeqLockArrayTest {

    @Test
    void slotsStartFromInitialValuesAtVersionZero() {
        SeqLockArray<Long> t = SeqLockArray.create(SeqLockImage.LONG, List.of(1L, 2L, 3L));
        SeqLockArray<Long>.Writer w = t.writer();
        SeqLockArray<Long>.Reader r = t.reader();
        assertEquals(3, t.size());
        assertEquals(3, w.size());
        assertEquals(3, r.size());
        for (int i = 0; i < 3; i++) {
            assertEquals(i + 1L, r.read(i));
            assertEquals(0, r.version(i));
            assertEquals(0, w.version(i));
            assertEquals(0, t.version(i));
            assertEquals(i + 1L, w.get(i));
        }
    }

    @Test
    void aWriteToOneSlotLeavesTheOthersUntouched() {
        SeqLockArray<long[]> t = SeqLockArray.create(SeqLockImage.longs(2), Collections.nCopies(4, new long[2]));
        SeqLockArray<long[]>.Writer w = t.writer();
        SeqLockArray<long[]>.Reader r = t.reader();
        w.update(2, v -> new long[] {9, v[1]});
        w.set(2, new long[] {9, 9});
        SeqLock.Versioned<long[]> v = r.readVersioned(2);
        assertArrayEquals(new long[] {9, 9}, v.value());
        assertEquals(2, v.version());
        for (int i : new int[] {0, 1, 3}) {
            SeqLock.Versioned<long[]> o = r.readVersioned(i);
            assertArrayEquals(new long[] {0, 0}, o.value());
            assertEquals(0, o.version());
        }
    }

    @Test
    void tryReadVariantsAndASecondReader() {
        SeqLockArray<Long> t = SeqLockArray.create(SeqLockImage.LONG, List.of(5L, 6L));
        SeqLockArray<Long>.Writer w = t.writer();
        SeqLockArray<Long>.Reader r = t.reader();
        SeqLockArray<Long>.Reader other = w.reader();
        w.set(1, 60L);
        assertEquals(60L, other.tryRead(1, 1));
        assertEquals(60L, r.tryReadOnce(1));
        assertNull(r.tryRead(0, 0));
        assertThrows(IllegalStateException.class, t::writer);
    }

    @Test
    void readAllReturnsEverySlotInOrder() {
        SeqLockArray<Long> t = SeqLockArray.create(SeqLockImage.LONG, Collections.nCopies(5, 0L));
        SeqLockArray<Long>.Writer w = t.writer();
        for (int i = 0; i < 5; i++) {
            w.set(i, i * 10L);
        }
        List<Long> out = new ArrayList<>(List.of(99L, 99L));
        t.reader().readAll(out);
        assertEquals(List.of(0L, 10L, 20L, 30L, 40L), out);
    }

    @Test
    void anEmptyArrayIsRefused() {
        assertThrows(IllegalArgumentException.class, () -> SeqLockArray.create(SeqLockImage.LONG, List.of()));
    }

    @Test
    void stressTwoReadersOnDistinctSlotsSeeOnlyTheirOwnWrites() throws InterruptedException {
        long updates = 200_000;
        SeqLockArray<long[]> t = SeqLockArray.create(SeqLockImage.longs(4), Collections.nCopies(8, new long[4]));
        SeqLockArray<long[]>.Writer w = t.writer();
        AtomicLong torn = new AtomicLong();
        List<Thread> readers = new ArrayList<>();
        for (int slot : new int[] {1, 6}) {
            Thread th = new Thread(() -> {
                SeqLockArray<long[]>.Reader r = t.reader();
                long last = 0;
                while (last < updates) {
                    SeqLock.Versioned<long[]> v = r.readVersioned(slot);
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
                }
            });
            th.start();
            readers.add(th);
        }
        long[] img = new long[4];
        for (long i = 1; i <= updates; i++) {
            Arrays.fill(img, i);
            w.set(1, img);
            w.set(6, img);
            if (i % 1000 == 0) {
                w.set(3, new long[] {i, 0, 0, 0});
            }
        }
        for (Thread th : readers) {
            th.join();
        }
        assertEquals(0, torn.get());
        assertEquals(0, t.version(0));
        assertEquals(updates, t.version(1));
        assertEquals(updates / 1000, t.version(3));
    }
}
