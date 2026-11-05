package com.submillisecond.recipes.seqlock;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.function.UnaryOperator;

/**
 * The double-buffered latch, {@code seqcount_latch} from the Linux kernel. Two copies; the
 * sequence's low bit says which one readers use. There is no write-in-progress state, so a
 * reader never waits on the writer and retries only if the writer flipped while it was copying.
 */
public final class SeqLatch<T> {

    private static final VarHandle SEQ;
    private static final VarHandle WORDS = MethodHandles.arrayElementVarHandle(long[].class);

    static {
        try {
            SEQ = MethodHandles.lookup().findVarHandle(SeqLock.Seq.class, "value", long.class);
        } catch (ReflectiveOperationException e) {
            throw new ExceptionInInitializerError(e);
        }
    }

    private final SeqLockImage<T> image;
    private final long[][] copies;
    private final SeqLock.Seq seq = new SeqLock.Seq();
    private final T initial;
    private boolean writerTaken;

    private SeqLatch(SeqLockImage<T> image, T initial) {
        if (image.words() <= 0) {
            throw new IllegalArgumentException("a seqlock image needs at least one word");
        }
        this.image = image;
        this.initial = initial;
        copies = new long[2][image.words()];
        for (long[] copy : copies) {
            image.store(initial, (i, w) -> copy[i] = w);
        }
    }

    public static <T> SeqLatch<T> create(SeqLockImage<T> image, T initial) {
        return new SeqLatch<>(image, initial);
    }

    public synchronized Writer writer() {
        if (writerTaken) {
            throw new IllegalStateException("a seqlatch has one writer");
        }
        writerTaken = true;
        return new Writer(initial);
    }

    public Reader reader() {
        return new Reader();
    }

    /** Writes published. Two sequence steps per write. */
    public long version() {
        return (long) SEQ.getAcquire(seq) >>> 1;
    }

    public final class Writer implements SeqLockImage.WordSink {
        private T shadow;
        private long[] target;

        private Writer(T initial) {
            shadow = initial;
        }

        public void set(T value) {
            shadow = value;
            publish();
        }

        public void update(UnaryOperator<T> f) {
            shadow = f.apply(shadow);
            publish();
        }

        public T get() {
            return shadow;
        }

        public long version() {
            return SeqLatch.this.version();
        }

        public Reader reader() {
            return new Reader();
        }

        // Flip readers to the other copy, rewrite the one they left, flip them onto it, then
        // bring the stale copy up to date for the next write.
        private void publish() {
            long s = (long) SEQ.getOpaque(seq);
            SEQ.setOpaque(seq, s + 1);
            VarHandle.releaseFence();
            target = copies[(int) (s & 1)];
            image.store(shadow, this);
            SEQ.setRelease(seq, s + 2);
            VarHandle.releaseFence();
            target = copies[(int) ((s + 1) & 1)];
            image.store(shadow, this);
        }

        @Override
        public void put(int i, long word) {
            WORDS.setOpaque(target, i, word);
        }
    }

    public final class Reader implements SeqLockImage.WordSource {
        private long[] current;
        private long lastSeq;

        private Reader() {}

        public T read() {
            for (;;) {
                T v = tryReadOnce();
                if (v != null) {
                    return v;
                }
                Thread.onSpinWait();
            }
        }

        /** A reader that lands between a write's two flips gets the previous value with the
         *  previous count, which is the consistent pair. */
        public SeqLock.Versioned<T> readVersioned() {
            T v = read();
            return new SeqLock.Versioned<>(v, lastSeq >>> 1);
        }

        public T tryRead(int maxAttempts) {
            for (int i = 0; i < maxAttempts; i++) {
                T v = tryReadOnce();
                if (v != null) {
                    return v;
                }
                Thread.onSpinWait();
            }
            return null;
        }

        public T tryReadOnce() {
            long s1 = (long) SEQ.getAcquire(seq);
            current = copies[(int) (s1 & 1)];
            T v = image.load(this);
            VarHandle.acquireFence();
            long s2 = (long) SEQ.getOpaque(seq);
            if (s1 != s2) {
                return null;
            }
            lastSeq = s1;
            return v;
        }

        public long lastVersion() {
            return lastSeq >>> 1;
        }

        public long version() {
            return SeqLatch.this.version();
        }

        @Override
        public long get(int i) {
            return (long) WORDS.getOpaque(current, i);
        }
    }
}
