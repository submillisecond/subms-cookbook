package com.submillisecond.recipes.seqlock;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Locale;

/**
 * Sample app: a tour of {@code subms-seqlock}, base API first, then each feature. A feed
 * handler publishes top of book, strategy threads read it. Output is byte-identical to the
 * Rust port's.
 */
public final class SampleApp {

    record TopOfBook(long bid, long bidQty, long ask, long askQty, long seq) {
        static TopOfBook at(long seq) {
            long mid = 100_000 + (seq % 7) * 10;
            return new TopOfBook(mid - 5, 100 + seq % 900, mid + 5, 100 + (seq * 3) % 900, seq);
        }

        boolean consistent() {
            return equals(at(seq));
        }
    }

    static final SeqLockImage<TopOfBook> IMAGE = new SeqLockImage<>() {
        @Override
        public int words() {
            return 5;
        }

        @Override
        public void store(TopOfBook v, WordSink put) {
            put.put(0, v.bid());
            put.put(1, v.bidQty());
            put.put(2, v.ask());
            put.put(3, v.askQty());
            put.put(4, v.seq());
        }

        @Override
        public TopOfBook load(WordSource get) {
            return new TopOfBook(get.get(0), get.get(1), get.get(2), get.get(3), get.get(4));
        }
    };

    public static void main(String[] args) throws InterruptedException {
        baseOneSymbolTwoReaders();
        arrayPerSymbolTable();
        latchNeverWaits();
        statsOnAReader();
    }

    static void baseOneSymbolTwoReaders() throws InterruptedException {
        System.out.println("== base: one symbol, one writer, two readers ==");
        long updates = 200_000;
        SeqLock<TopOfBook> lock = SeqLock.create(IMAGE, TopOfBook.at(0));
        SeqLock<TopOfBook>.Writer writer = lock.writer();

        long[] torn = new long[2];
        List<Thread> readers = new ArrayList<>();
        for (int t = 0; t < 2; t++) {
            int slot = t;
            Thread th = new Thread(() -> {
                SeqLock<TopOfBook>.Reader r = lock.reader();
                long last = 0;
                while (last < updates) {
                    SeqLock.Versioned<TopOfBook> v = r.readVersioned();
                    if (!v.value().consistent() || v.version() != v.value().seq()) {
                        torn[slot]++;
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

        SeqLock<TopOfBook>.Reader reader = lock.reader();
        TopOfBook last = reader.read();
        System.out.println("  writer published " + writer.version() + " updates");
        System.out.println("  torn snapshots across both readers: " + (torn[0] + torn[1]));
        System.out.println(String.format(Locale.ROOT, "  final top: %d x %d / %d x %d (seq %d)",
                last.bidQty(), last.bid(), last.ask(), last.askQty(), last.seq()));
        System.out.println("  a bounded read on an idle lock succeeds first try: " + (reader.tryRead(1) != null));
    }

    static void arrayPerSymbolTable() {
        System.out.println("== array: a per-symbol table ==");
        String[] symbols = {"ESZ5", "NQZ5", "CLZ5", "GCZ5"};
        SeqLockArray<TopOfBook> table = SeqLockArray.create(IMAGE, Collections.nCopies(4, TopOfBook.at(0)));
        SeqLockArray<TopOfBook>.Writer writer = table.writer();
        SeqLockArray<TopOfBook>.Reader reader = table.reader();

        for (long seq = 1; seq <= 1_000; seq++) {
            writer.set(0, TopOfBook.at(seq));
        }
        writer.update(2, top -> new TopOfBook(top.bid(), 42, top.ask(), top.askQty(), 1));

        for (int i = 0; i < symbols.length; i++) {
            SeqLock.Versioned<TopOfBook> v = reader.readVersioned(i);
            System.out.println("  " + symbols[i] + ": version " + v.version() + ", bid_qty "
                    + v.value().bidQty() + ", seq " + v.value().seq());
        }
        List<TopOfBook> all = new ArrayList<>();
        reader.readAll(all);
        System.out.println("  read_all returned " + all.size() + " slots; slot 1 and 3 were never written");
    }

    static void latchNeverWaits() throws InterruptedException {
        System.out.println("== latch: readers never wait on the writer ==");
        long updates = 200_000;
        SeqLatch<TopOfBook> latch = SeqLatch.create(IMAGE, TopOfBook.at(0));
        SeqLatch<TopOfBook>.Writer writer = latch.writer();

        long[] torn = {0};
        Thread strategy = new Thread(() -> {
            SeqLatch<TopOfBook>.Reader r = latch.reader();
            long last = 0;
            while (last < updates) {
                TopOfBook top = r.read();
                if (!top.consistent()) {
                    torn[0]++;
                }
                last = top.seq();
            }
        });
        strategy.start();
        for (long seq = 1; seq <= updates; seq++) {
            writer.set(TopOfBook.at(seq));
        }
        strategy.join();

        System.out.println("  writer published " + writer.version() + " updates");
        System.out.println("  torn snapshots: " + torn[0]);
        SeqLatch<TopOfBook>.Reader reader = latch.reader();
        SeqLock.Versioned<TopOfBook> v = reader.readVersioned();
        System.out.println("  idle read: seq " + v.value().seq() + " at version " + v.version());
        System.out.println("  two copies, both current after every write: " + reader.read().equals(writer.get()));
    }

    static void statsOnAReader() {
        System.out.println("== stats: retry counters on a reader ==");
        SeqLock<TopOfBook> lock = SeqLock.create(IMAGE, TopOfBook.at(0));
        SeqLock<TopOfBook>.Writer writer = lock.writer();
        InstrumentedReader<TopOfBook> counted = new InstrumentedReader<>(lock.reader());

        for (long seq = 1; seq <= 1_000; seq++) {
            writer.set(TopOfBook.at(seq));
            counted.read();
        }
        ReadStats.Snapshot s = counted.snapshot();
        System.out.println(String.format(Locale.ROOT, "  idle lock: reads %d, retries %d, worst %d, rate %.3f",
                s.reads(), s.retries(), s.maxRetries(), s.retryRate()));
        counted.stats().reset();
        System.out.println("  after reset: reads " + counted.snapshot().reads());
    }
}
