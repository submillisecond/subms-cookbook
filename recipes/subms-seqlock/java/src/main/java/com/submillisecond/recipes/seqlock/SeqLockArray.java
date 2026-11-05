package com.submillisecond.recipes.seqlock;

import java.util.List;
import java.util.function.UnaryOperator;

/** N independent slots under one writer; a write to one slot never makes a reader of another
 *  retry. */
public final class SeqLockArray<T> {

    private final SeqLock<T>[] slots;
    private boolean writerTaken;

    @SuppressWarnings("unchecked")
    private SeqLockArray(SeqLockImage<T> image, List<T> initial) {
        if (initial.isEmpty()) {
            throw new IllegalArgumentException("a seqlock array needs at least one slot");
        }
        slots = new SeqLock[initial.size()];
        for (int i = 0; i < slots.length; i++) {
            slots[i] = SeqLock.create(image, initial.get(i));
        }
    }

    public static <T> SeqLockArray<T> create(SeqLockImage<T> image, List<T> initial) {
        return new SeqLockArray<>(image, initial);
    }

    public synchronized Writer writer() {
        if (writerTaken) {
            throw new IllegalStateException("a seqlock array has one writer");
        }
        writerTaken = true;
        return new Writer();
    }

    public Reader reader() {
        return new Reader();
    }

    public int size() {
        return slots.length;
    }

    public long version(int idx) {
        return slots[idx].version();
    }

    public final class Writer {
        private final SeqLock<T>.Writer[] writers;

        @SuppressWarnings("unchecked")
        private Writer() {
            writers = new SeqLock.Writer[slots.length];
            for (int i = 0; i < slots.length; i++) {
                writers[i] = slots[i].writer();
            }
        }

        public void set(int idx, T value) {
            writers[idx].set(value);
        }

        public void update(int idx, UnaryOperator<T> f) {
            writers[idx].update(f);
        }

        public T get(int idx) {
            return writers[idx].get();
        }

        public long version(int idx) {
            return slots[idx].version();
        }

        public int size() {
            return slots.length;
        }

        public Reader reader() {
            return new Reader();
        }
    }

    public final class Reader {
        private final SeqLock<T>.Reader[] readers;

        @SuppressWarnings("unchecked")
        private Reader() {
            readers = new SeqLock.Reader[slots.length];
            for (int i = 0; i < slots.length; i++) {
                readers[i] = slots[i].reader();
            }
        }

        public T read(int idx) {
            return readers[idx].read();
        }

        public SeqLock.Versioned<T> readVersioned(int idx) {
            return readers[idx].readVersioned();
        }

        public T tryRead(int idx, int maxAttempts) {
            return readers[idx].tryRead(maxAttempts);
        }

        public T tryReadOnce(int idx) {
            return readers[idx].tryReadOnce();
        }

        /** Every slot, each individually consistent. Not a snapshot across slots. */
        public void readAll(List<T> out) {
            out.clear();
            for (SeqLock<T>.Reader r : readers) {
                out.add(r.read());
            }
        }

        public long version(int idx) {
            return slots[idx].version();
        }

        public int size() {
            return slots.length;
        }
    }
}
