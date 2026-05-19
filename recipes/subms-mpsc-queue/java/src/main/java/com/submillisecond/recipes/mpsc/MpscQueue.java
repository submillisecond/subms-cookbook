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
}
