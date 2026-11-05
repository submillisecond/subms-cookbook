package com.submillisecond.recipes.seqlock;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.function.UnaryOperator;

/**
 * Sequence lock: one writer publishes a snapshot, any number of readers take it, no lock on
 * either side. Words go through opaque accesses, so a torn read is a value the sequence check
 * discards rather than a data race.
 *
 * <p>{@link #writer()} may be called once. {@link #reader()} is cheap; take one per thread,
 * since a reader keeps the version of its last read.
 */
public final class SeqLock<T> {

    private static final VarHandle SEQ;
    private static final VarHandle WORDS = MethodHandles.arrayElementVarHandle(long[].class);

    static {
        try {
            SEQ = MethodHandles.lookup().findVarHandle(Seq.class, "value", long.class);
        } catch (ReflectiveOperationException e) {
            throw new ExceptionInInitializerError(e);
        }
    }

    // Padded so the sequence never shares a line with the words or a neighbour.
    @SuppressWarnings("unused")
    static final class Seq {
        private long p00, p01, p02, p03, p04, p05, p06, p07;
        volatile long value;
        private long p10, p11, p12, p13, p14, p15, p16, p17;
    }

    public record Versioned<T>(T value, long version) {}

    private final SeqLockImage<T> image;
    private final long[] words;
    private final Seq seq = new Seq();
    private final T initial;
    private boolean writerTaken;

    private SeqLock(SeqLockImage<T> image, T initial) {
        if (image.words() <= 0) {
            throw new IllegalArgumentException("a seqlock image needs at least one word");
        }
        this.image = image;
        this.initial = initial;
        this.words = new long[image.words()];
        image.store(initial, (i, w) -> words[i] = w);
    }

    public static <T> SeqLock<T> create(SeqLockImage<T> image, T initial) {
        return new SeqLock<>(image, initial);
    }

    /** The one writer. A second call throws. */
    public synchronized Writer writer() {
        if (writerTaken) {
            throw new IllegalStateException("a seqlock has one writer");
        }
        writerTaken = true;
        return new Writer(initial);
    }

    public Reader reader() {
        return new Reader();
    }

    public long version() {
        return (long) SEQ.getAcquire(seq) >>> 1;
    }

    public boolean isWriting() {
        return ((long) SEQ.getAcquire(seq) & 1) == 1;
    }

    public final class Writer implements SeqLockImage.WordSink {
        private T shadow;

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

        /** The current value, from the writer's copy. No fences. */
        public T get() {
            return shadow;
        }

        public long version() {
            return SeqLock.this.version();
        }

        public Reader reader() {
            return new Reader();
        }

        // Odd, words, even. The release fence stops a word becoming visible ahead of the odd
        // sequence; the release store covers the words before it.
        private void publish() {
            long s = (long) SEQ.getOpaque(seq);
            SEQ.setOpaque(seq, s + 1);
            VarHandle.releaseFence();
            image.store(shadow, this);
            SEQ.setRelease(seq, s + 2);
        }

        @Override
        public void put(int i, long word) {
            WORDS.setOpaque(words, i, word);
        }
    }

    public final class Reader implements SeqLockImage.WordSource {
        private long lastSeq;

        private Reader() {}

        /** A consistent snapshot, retrying until one is taken. */
        public T read() {
            for (;;) {
                T v = tryReadOnce();
                if (v != null) {
                    return v;
                }
                Thread.onSpinWait();
            }
        }

        public Versioned<T> readVersioned() {
            T v = read();
            return new Versioned<>(v, lastSeq >>> 1);
        }

        /** At most {@code maxAttempts} tries, then {@code null}. */
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

        /** One attempt: the snapshot, or {@code null} if a write was in flight or landed during
         *  the copy. {@link #lastVersion()} is the version of a successful attempt. */
        public T tryReadOnce() {
            long s1 = (long) SEQ.getAcquire(seq);
            if ((s1 & 1) == 1) {
                return null;
            }
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
            return SeqLock.this.version();
        }

        public boolean isWriting() {
            return SeqLock.this.isWriting();
        }

        @Override
        public long get(int i) {
            return (long) WORDS.getOpaque(words, i);
        }
    }
}
