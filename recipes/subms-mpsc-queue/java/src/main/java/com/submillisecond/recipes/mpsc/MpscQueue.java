package com.submillisecond.recipes.mpsc;

import java.lang.invoke.MethodHandles;
import java.lang.invoke.VarHandle;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Vyukov-style multi-producer single-consumer linked queue.
 *
 * <p>Producers swap the head pointer atomically; the consumer drains by
 * walking {@code next} pointers from the tail. The dangling-tail window
 * (between {@code swap(head, new)} and {@code prev.next = new}) is the
 * load-bearing detail: {@link #tryPoll()} returns {@code null} when the
 * window is open, and {@link #isInconsistent()} distinguishes "truly empty"
 * from "producer mid-publish" so the caller can spin or back off.
 *
 * @param <T> element type; nulls are not permitted.
 */
public final class MpscQueue<T> {

    private static final class Node<T> {
        volatile Node<T> next;
        T value;
        static final VarHandle NEXT;
        static {
            try {
                NEXT = MethodHandles.lookup().findVarHandle(Node.class, "next", Node.class);
            } catch (ReflectiveOperationException e) {
                throw new ExceptionInInitializerError(e);
            }
        }
    }

    private final Node<T> stub = new Node<>();
    private final AtomicReference<Node<T>> head;
    private Node<T> tail = stub;

    public MpscQueue() {
        this.head = new AtomicReference<>(stub);
    }

    /** Multi-producer push. Wait-free once the node is allocated. */
    public void push(T value) {
        if (value == null) throw new NullPointerException();
        Node<T> n = new Node<>();
        n.value = value;
        Node<T> prev = head.getAndSet(n);
        Node.NEXT.setRelease(prev, n);
    }

    /**
     * Publish a whole run of values with a single head swap.
     *
     * <p>The producer links the nodes together privately before publication,
     * so the chain costs one {@code getAndSet} and one release-store no matter
     * how many items it carries. Returns the number published; a zero-length
     * run touches no shared state at all.
     *
     * <p>Items keep their array order relative to each other, and the run is
     * published atomically: a consumer either sees none of it or sees the
     * whole chain reachable from the node it links onto.
     *
     * @param values the run to publish
     * @param len how many leading entries of {@code values} to take
     * @return the number of items published
     */
    public int pushBatch(T[] values, int len) {
        int n = Math.min(len, values.length);
        if (n <= 0) return 0;
        Node<T> first = null;
        Node<T> last = null;
        for (int i = 0; i < n; i++) {
            T value = values[i];
            if (value == null) throw new NullPointerException();
            Node<T> node = new Node<>();
            node.value = value;
            if (first == null) {
                first = node;
            } else {
                // Plain store: the chain is thread-private until the
                // release-store below publishes it.
                Node.NEXT.set(last, node);
            }
            last = node;
        }
        Node<T> prev = head.getAndSet(last);
        Node.NEXT.setRelease(prev, first);
        return n;
    }

    /** Publish every entry of {@code values}. */
    public int pushBatch(T[] values) {
        return pushBatch(values, values.length);
    }

    /**
     * Consume one entry. Returns {@code null} when the queue is empty OR when
     * a producer is mid-publish (the dangling-tail window). Use
     * {@link #isInconsistent()} to distinguish.
     *
     * <p>Single consumer only.
     */
    public T tryPoll() {
        Node<T> t = tail;
        Node<T> next = (Node<T>) Node.NEXT.getAcquire(t);
        if (t == stub) {
            if (next == null) return null;
            tail = next;
            T v = next.value;
            next.value = null; // release reference for GC
            return v;
        }
        if (next != null) {
            tail = next;
            T v = next.value;
            next.value = null;
            return v;
        }
        return null;
    }

    /**
     * After a {@code null} from {@link #tryPoll()}, this distinguishes a
     * producer-in-progress (dangling tail) from a truly empty queue.
     */
    public boolean isInconsistent() {
        Node<T> t = tail;
        Node<T> next = (Node<T>) Node.NEXT.getAcquire(t);
        if (t == stub && next == null) {
            return head.get() != stub;
        }
        if (next == null) {
            return head.get() != t;
        }
        return false;
    }

    /**
     * Return the next value without consuming it, or {@code null} when the
     * queue is drained or a producer is mid-publish. Matches JCTools'
     * {@code relaxedPeek}; call {@link #isEmpty()} to tell the two apart.
     *
     * <p>Consumer-side only.
     */
    @SuppressWarnings("unchecked")
    public T peek() {
        Node<T> t = tail;
        Node<T> next = (Node<T>) Node.NEXT.getAcquire(t);
        return next == null ? null : next.value;
    }

    /**
     * True only when the queue is genuinely drained. A producer inside the
     * dangling-tail window reads as non-empty, because its item is already
     * committed to the chain even though the link is not written yet.
     *
     * <p>Consumer-side only.
     */
    @SuppressWarnings("unchecked")
    public boolean isEmpty() {
        Node<T> t = tail;
        Node<T> next = (Node<T>) Node.NEXT.getAcquire(t);
        return next == null && head.get() == t;
    }

    /**
     * Count the linked items. O(n) in the backlog, and an estimate under a
     * live producer, which is the contract JCTools' {@code size()} carries on
     * its linked queues. Use it for alarms and sizing, not on the hot path.
     *
     * <p>Consumer-side only.
     */
    @SuppressWarnings("unchecked")
    public int size() {
        int n = 0;
        Node<T> node = tail;
        while (true) {
            Node<T> next = (Node<T>) Node.NEXT.getAcquire(node);
            if (next == null) return n;
            n++;
            node = next;
        }
    }

    /**
     * Drop everything the consumer can currently reach and return the count.
     *
     * <p>Best-effort: producers keep publishing throughout, so this is not a
     * barrier and the queue is not guaranteed empty on return. It stops at the
     * first dangling-tail window rather than spinning through it.
     *
     * <p>Consumer-side only.
     */
    public int clear() {
        int n = 0;
        while (tryPoll() != null) n++;
        return n;
    }
}
