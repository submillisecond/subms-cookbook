package com.submillisecond.recipes.events;

import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.atomic.AtomicLong;

/**
 * The dispatcher. Sync: listeners run inline on the emitter. Async: events go to
 * a daemon thread over a queue, so a slow listener never blocks the emitter. The
 * async queue is unbounded by default; use {@link #bounded} to cap it and pick an
 * {@link OverflowPolicy}.
 */
public final class EventDispatcher {
    private final DispatchMode mode;
    private final Integer capacity; // null = unbounded
    private final OverflowPolicy policy;
    private final List<EventListener> listeners = new ArrayList<>();
    private final AtomicLong dropped = new AtomicLong();
    private volatile BlockingQueue<Event> queue;
    private volatile boolean stopped;
    private Thread thread;

    public EventDispatcher(DispatchMode mode) {
        this(mode, null, OverflowPolicy.BLOCK);
    }

    private EventDispatcher(DispatchMode mode, Integer capacity, OverflowPolicy policy) {
        this.mode = mode;
        this.capacity = capacity;
        this.policy = policy;
    }

    public static EventDispatcher sync() {
        return new EventDispatcher(DispatchMode.SYNC);
    }

    public static EventDispatcher asynchronous() {
        return new EventDispatcher(DispatchMode.ASYNC);
    }

    /** Off-thread dispatcher with a bounded queue + overflow policy. */
    public static EventDispatcher bounded(int capacity, OverflowPolicy policy) {
        return new EventDispatcher(DispatchMode.ASYNC, Math.max(1, capacity), policy);
    }

    public DispatchMode mode() {
        return mode;
    }

    /** Count of events dropped under DROP_NEWEST / DROP_OLDEST. */
    public long dropped() {
        return dropped.get();
    }

    public int listenerCount() {
        synchronized (listeners) {
            return listeners.size();
        }
    }

    public EventDispatcher addListener(EventListener listener) {
        synchronized (listeners) {
            listeners.add(listener);
        }
        if (mode == DispatchMode.ASYNC) {
            ensureThread();
        }
        return this;
    }

    public EventDispatcher addBridge(EventBridge bridge) {
        return addListener(new BridgeListener(bridge));
    }

    private synchronized void ensureThread() {
        if (thread != null) {
            return;
        }
        queue = capacity == null ? new LinkedBlockingQueue<>() : new LinkedBlockingQueue<>(capacity);
        BlockingQueue<Event> q = queue;
        thread = new Thread(() -> {
            while (!stopped) {
                Event event;
                try {
                    event = q.take();
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                    return;
                }
                dispatchToListeners(event);
            }
        }, "subms-events-dispatch");
        thread.setDaemon(true);
        thread.start();
    }

    private void dispatchToListeners(Event event) {
        List<EventListener> snapshot;
        synchronized (listeners) {
            snapshot = new ArrayList<>(listeners);
        }
        for (EventListener l : snapshot) {
            l.onEvent(event);
        }
    }

    void dispatch(Event event) {
        if (mode == DispatchMode.SYNC) {
            dispatchToListeners(event);
            return;
        }
        BlockingQueue<Event> q = queue;
        if (q == null) {
            return;
        }
        if (capacity == null) {
            q.offer(event);
            return;
        }
        switch (policy) {
            case BLOCK -> {
                try {
                    q.put(event);
                } catch (InterruptedException e) {
                    Thread.currentThread().interrupt();
                }
            }
            case DROP_NEWEST -> {
                if (!q.offer(event)) {
                    dropped.incrementAndGet();
                }
            }
            case DROP_OLDEST -> {
                if (!q.offer(event)) {
                    q.poll();
                    dropped.incrementAndGet();
                    q.offer(event);
                }
            }
        }
    }

    public void emit(Event event) {
        dispatch(event);
    }

    public EmitHandle handle() {
        return new EmitHandle(this);
    }

    public synchronized void stop() {
        if (thread != null) {
            stopped = true;
            thread.interrupt();
            try {
                thread.join();
            } catch (InterruptedException e) {
                Thread.currentThread().interrupt();
            }
            thread = null;
            queue = null;
        }
    }
}
