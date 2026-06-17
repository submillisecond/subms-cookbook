package com.submillisecond.recipes.tscdc;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.spsc.SpscRingBuffer;

/**
 * The read end of one subscriber's ring. Wraps the {@link SpscRingBuffer}
 * consumer half; the matching producer lives in the
 * {@link TsObservableCollection}. SPSC means exactly one reader - keep the
 * subscription single-thread-owned.
 *
 * @param <T> the series value type
 */
public final class TsSubscription<T> {

    private final SpscRingBuffer<TsChangeEvent<T>>.Consumer rx;

    TsSubscription(SpscRingBuffer<TsChangeEvent<T>>.Consumer rx) {
        this.rx = rx;
    }

    /** Pop the next event, or {@code null} if the ring is currently empty. */
    public TsChangeEvent<T> tryRecv() {
        return rx.tryPop();
    }

    /**
     * Drain every currently-buffered event in FIFO order. Stops at the first
     * empty pop; events published after the drain starts are seen on the next
     * call.
     */
    public List<TsChangeEvent<T>> drain() {
        List<TsChangeEvent<T>> out = new ArrayList<>();
        TsChangeEvent<T> ev;
        while ((ev = rx.tryPop()) != null) {
            out.add(ev);
        }
        return out;
    }
}
