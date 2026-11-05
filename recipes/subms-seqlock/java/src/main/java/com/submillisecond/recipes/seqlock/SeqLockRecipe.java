package com.submillisecond.recipes.seqlock;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stages: {@code write} on the writer thread, {@code read} on a reader thread running
 *  concurrently, so a read's sample includes any retry the writer caused. */
public final class SeqLockRecipe implements SubMsRecipe {

    record Top(long bid, long bidQty, long ask, long askQty, long seq) {}

    static final SeqLockImage<Top> TOP = new SeqLockImage<>() {
        @Override
        public int words() {
            return 5;
        }

        @Override
        public void store(Top v, WordSink put) {
            put.put(0, v.bid());
            put.put(1, v.bidQty());
            put.put(2, v.ask());
            put.put(3, v.askQty());
            put.put(4, v.seq());
        }

        @Override
        public Top load(WordSource get) {
            return new Top(get.get(0), get.get(1), get.get(2), get.get(3), get.get(4));
        }
    };

    static volatile long sink;

    @Override
    public String name() {
        return "seqlock";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        SeqLock<Top> lock = SeqLock.create(TOP, new Top(100_000_000, 300, 100_010_000, 500, 0));
        SeqLock<Top>.Writer writer = lock.writer();
        SeqLock<Top>.Reader warm = lock.reader();

        for (long i = 0; i < warmup; i++) {
            writer.set(new Top(100_000_000 + (i % 100) * 10_000, 300, 100_010_000, 500, i));
            sink = warm.read().seq();
        }

        long[] readSamples = new long[entries];
        Thread consumer = new Thread(() -> {
            SeqLock<Top>.Reader reader = lock.reader();
            long acc = 0;
            for (int k = 0; k < entries; k++) {
                long t0 = SubMsTimer.nanosNow();
                Top top = reader.read();
                readSamples[k] = SubMsTimer.nanosNow() - t0;
                acc ^= top.seq();
            }
            sink = acc;
        });
        consumer.start();

        long[] writeSamples = new long[entries];
        for (int k = 0; k < entries; k++) {
            long i = k;
            long t0 = SubMsTimer.nanosNow();
            writer.set(new Top(100_000_000 + (i % 100) * 10_000, 100 + (i % 7) * 100, 100_010_000, 500, i));
            writeSamples[k] = SubMsTimer.nanosNow() - t0;
        }
        try {
            consumer.join();
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RuntimeException(e);
        }

        SubMsPerfHarness.Stage w = h.stage("write", entries).withKind(SubMsStageKind.HOT_PATH);
        for (long ns : writeSamples) {
            w.record(ns);
        }
        SubMsPerfHarness.Stage r = h.stage("read", entries).withKind(SubMsStageKind.HOT_PATH);
        for (long ns : readSamples) {
            r.record(ns);
        }
        h.meta("image_words", "5");
        h.meta("readers", "1");
    }
}
