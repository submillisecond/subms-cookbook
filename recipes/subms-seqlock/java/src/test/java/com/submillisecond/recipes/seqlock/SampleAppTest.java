package com.submillisecond.recipes.seqlock;

import com.submillisecond.recipes.seqlock.SampleApp.TopOfBook;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.PrintStream;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.concurrent.atomic.AtomicLong;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        SeqLock<long[]> lock = SeqLock.create(SeqLockImage.longs(4), new long[] {10_025, 300, 10_026, 500});
        SeqLock<long[]>.Writer writer = lock.writer();
        SeqLock<long[]>.Reader reader = lock.reader();

        writer.update(top -> new long[] {10_026, 200, top[2], top[3]});   // bid moves up a tick
        long[] snap = reader.read();                                         // every field from the same write
        assertEquals(10_026, snap[0]);
        assertEquals(200, snap[1]);
        assertEquals(1, reader.version());
        // quickstart:end
    }

    @Test
    void baseTwoReadersTakeOnlyConsistentSnapshots() throws InterruptedException {
        long updates = 50_000;
        SeqLock<TopOfBook> lock = SeqLock.create(SampleApp.IMAGE, TopOfBook.at(0));
        SeqLock<TopOfBook>.Writer writer = lock.writer();
        AtomicLong torn = new AtomicLong();
        List<Thread> readers = new ArrayList<>();
        for (int t = 0; t < 2; t++) {
            Thread th = new Thread(() -> {
                SeqLock<TopOfBook>.Reader r = lock.reader();
                long last = 0;
                while (last < updates) {
                    SeqLock.Versioned<TopOfBook> v = r.readVersioned();
                    if (!v.value().consistent() || v.version() != v.value().seq()) {
                        torn.incrementAndGet();
                    }
                    last = v.value().seq();
                }
            });
            th.start();
            readers.add(th);
        }
        for (long seq = 1; seq <= updates; seq++) {
            writer.set(TopOfBook.at(seq));
        }
        for (Thread th : readers) {
            th.join();
        }
        assertEquals(0, torn.get());
        assertEquals(updates, writer.version());
        assertEquals(TopOfBook.at(updates), lock.reader().read());
        assertNotNull(lock.reader().tryRead(1));
    }

    @Test
    void arraySectionIsolatesSymbols() {
        SeqLockArray<TopOfBook> table = SeqLockArray.create(SampleApp.IMAGE, Collections.nCopies(4, TopOfBook.at(0)));
        SeqLockArray<TopOfBook>.Writer writer = table.writer();
        SeqLockArray<TopOfBook>.Reader reader = table.reader();
        for (long seq = 1; seq <= 1_000; seq++) {
            writer.set(0, TopOfBook.at(seq));
        }
        writer.update(2, top -> new TopOfBook(top.bid(), 42, top.ask(), top.askQty(), 1));
        assertEquals(1_000, reader.version(0));
        assertEquals(0, reader.version(1));
        assertEquals(1, reader.version(2));
        assertEquals(0, reader.version(3));
        assertEquals(42, reader.read(2).bidQty());
        List<TopOfBook> all = new ArrayList<>();
        reader.readAll(all);
        assertEquals(4, all.size());
        assertEquals(TopOfBook.at(0), all.get(1));
    }

    @Test
    void latchSectionReadsAreConsistentAndBothCopiesCurrent() throws InterruptedException {
        long updates = 50_000;
        SeqLatch<TopOfBook> latch = SeqLatch.create(SampleApp.IMAGE, TopOfBook.at(0));
        SeqLatch<TopOfBook>.Writer writer = latch.writer();
        AtomicLong torn = new AtomicLong();
        Thread th = new Thread(() -> {
            SeqLatch<TopOfBook>.Reader r = latch.reader();
            long last = 0;
            while (last < updates) {
                SeqLock.Versioned<TopOfBook> v = r.readVersioned();
                if (!v.value().consistent() || (v.version() != v.value().seq() && v.version() + 1 != v.value().seq())) {
                    torn.incrementAndGet();
                }
                last = v.value().seq();
            }
        });
        th.start();
        for (long seq = 1; seq <= updates; seq++) {
            writer.set(TopOfBook.at(seq));
        }
        th.join();
        assertEquals(0, torn.get());
        SeqLatch<TopOfBook>.Reader reader = latch.reader();
        SeqLock.Versioned<TopOfBook> v = reader.readVersioned();
        assertEquals(TopOfBook.at(updates), v.value());
        assertEquals(updates, v.version());
        assertEquals(writer.get(), reader.read());
    }

    @Test
    void statsSectionCountsIdleReadsAndResets() {
        SeqLock<TopOfBook> lock = SeqLock.create(SampleApp.IMAGE, TopOfBook.at(0));
        SeqLock<TopOfBook>.Writer writer = lock.writer();
        InstrumentedReader<TopOfBook> counted = new InstrumentedReader<>(lock.reader());
        for (long seq = 1; seq <= 1_000; seq++) {
            writer.set(TopOfBook.at(seq));
            counted.read();
        }
        assertEquals(new ReadStats.Snapshot(1_000, 0, 0), counted.snapshot());
        counted.stats().reset();
        assertEquals(0, counted.snapshot().reads());
    }

    @Test
    void mainRunsEverySectionAndPrintsDeterministically() throws InterruptedException {
        PrintStream orig = System.out;
        ByteArrayOutputStream a = new ByteArrayOutputStream();
        ByteArrayOutputStream b = new ByteArrayOutputStream();
        try {
            System.setOut(new PrintStream(a, true, StandardCharsets.UTF_8));
            SampleApp.main(new String[0]);
            System.setOut(new PrintStream(b, true, StandardCharsets.UTF_8));
            SampleApp.main(new String[0]);
        } finally {
            System.setOut(orig);
        }
        String out = a.toString(StandardCharsets.UTF_8);
        assertEquals(out, b.toString(StandardCharsets.UTF_8));
        assertTrue(out.contains("torn snapshots across both readers: 0"));
        assertTrue(out.contains("CLZ5: version 1, bid_qty 42, seq 1"));
        assertTrue(out.contains("two copies, both current after every write: true"));
        assertTrue(out.contains("idle lock: reads 1000, retries 0, worst 0, rate 0.000"));
    }
}
