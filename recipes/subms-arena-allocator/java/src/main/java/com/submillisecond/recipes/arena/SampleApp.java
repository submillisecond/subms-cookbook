package com.submillisecond.recipes.arena;

import com.submillisecond.recipes.arena.features.AlignedArena;
import com.submillisecond.recipes.arena.features.GrowableArena;
import com.submillisecond.recipes.arena.features.Slot;
import com.submillisecond.recipes.arena.features.StatsArena;
import com.submillisecond.recipes.arena.features.TypedArena;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * Sample app: a tour of {@code subms-arena-allocator} for per-tick / per-request
 * scratch, base API first, then each feature variant. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.arena.SampleApp}
 *
 * <ul>
 *   <li>base     - per-tick order-book scratch, reset between ticks
 *   <li>typed    - TypedArena&lt;Level&gt;: slot handles, freed slots reused
 *   <li>growable - a deep-book tick outgrows the buffer; the arena grows
 *   <li>stats    - lifetime counters to size the arena from real load
 *   <li>aligned  - cache-line-aligned offset scratch for a price scan
 * </ul>
 */
public final class SampleApp {

    /** Per-level scratch value held in the typed arena's slots. */
    record Level(long priceTicks, int qty) {}

    public static void main(String[] args) {
        basePerTickScratch();
        typedSnapshot();
        growableDeepBook();
        statsSizing();
        alignedPriceScan();
    }

    /** Base API: each tick's price levels land in one fixed {@code byte[]} at
     *  aligned offsets; we read them back to compute the mid and the
     *  resting-quantity imbalance, then reset for the next tick. The buffer is
     *  sized once, so the steady state never allocates. */
    static void basePerTickScratch() {
        System.out.println("== base: per-tick order-book scratch ==");
        BumpArena scratch = new BumpArena(4096);
        ByteBuffer buf = ByteBuffer.wrap(scratch.bytes());
        int cap = scratch.capacity();

        // Flattened (price, qty, side) triples per tick; side 0 = bid, 1 = ask.
        int[][] ticks = {
            {9998, 5, 0, 9997, 8, 0, 10002, 4, 1, 10003, 9, 1},
            {9999, 3, 0, 10001, 7, 1},
        };

        for (int t = 0; t < ticks.length; t++) {
            int[] updates = ticks[t];
            long bestBid = 0, bestAsk = Long.MAX_VALUE, bidQty = 0, askQty = 0;
            for (int i = 0; i < updates.length; i += 3) {
                int off = scratch.allocate(12, 8);
                buf.putLong(off, updates[i]);
                buf.putInt(off + 8, updates[i + 1]);
                long price = buf.getLong(off);
                int qty = buf.getInt(off + 8);
                if (updates[i + 2] == 0) {
                    bestBid = Math.max(bestBid, price);
                    bidQty += qty;
                } else {
                    bestAsk = Math.min(bestAsk, price);
                    askQty += qty;
                }
            }
            long mid = (bestBid + bestAsk) / 2;
            long imbalance = bidQty - askQty;
            System.out.println("  tick " + t + ": " + (updates.length / 3)
                + " levels, mid=" + mid + " imbalance=" + imbalance
                + " used=" + scratch.used() + "B");
            if (scratch.used() <= 0) throw new AssertionError("levels consumed scratch");
            scratch.reset();
            if (scratch.used() != 0) throw new AssertionError("reset rewinds the cursor");
            if (scratch.capacity() != cap) throw new AssertionError("no reallocation between ticks");
        }
        System.out.println("  -> steady-state buffer stays at " + cap + "B across all ticks");
    }

    /** typed: {@link TypedArena} when every scratch object is the same type.
     *  alloc() hands back an opaque {@link Slot} that get() reads, a cancelled
     *  level is freed back to the arena, and the next alloc() takes that slot
     *  instead of consuming a fresh one - so an order cache that churns inside
     *  one tick stops advancing the high-water mark. */
    static void typedSnapshot() {
        System.out.println("\n== typed: per-tick levels with slot reuse ==");
        TypedArena<Level> book = new TypedArena<>(64);
        int[][] levels = {{9998, 5}, {9997, 8}, {10002, 4}, {10003, 9}};
        List<Slot> live = new ArrayList<>();
        for (int[] lv : levels) {
            live.add(book.alloc(new Level(lv[0], lv[1])));
        }
        long resting = 0, top = 0;
        for (Slot s : live) {
            resting += book.get(s).qty();
            top = Math.max(top, book.get(s).priceTicks());
        }
        System.out.println("  " + book.len() + " levels, " + resting + " resting, top " + top);
        if (book.len() != 4) throw new AssertionError("four levels");
        if (resting != 26) throw new AssertionError("resting qty");

        Slot cancelled = live.remove(live.size() - 1);
        int freedIndex = cancelled.index();
        book.free(cancelled);
        Slot replacement = book.alloc(new Level(10_004, 2));
        System.out.println("  cancelled level " + freedIndex + " reused by the replacement: "
            + (replacement.index() == freedIndex) + " (" + book.reuseHits() + " reuse hits)");
        if (replacement.index() != freedIndex) throw new AssertionError("freed slot came back");
        if (book.reuseHits() != 1) throw new AssertionError("one reuse hit");
        if (book.len() != 4) throw new AssertionError("cancel + replace is footprint-neutral");

        book.reset();
        if (!book.isEmpty()) throw new AssertionError("reset recycles the storage");
    }

    /** growable: a deep-book tick overflows the buffer; {@link GrowableArena}
     *  opens a larger one instead of failing. reset() retains the largest
     *  buffer, so the next tick runs grow-free. */
    static void growableDeepBook() {
        System.out.println("\n== growable: a deep-book tick that outgrows the buffer ==");
        GrowableArena scratch = new GrowableArena(256);
        for (int i = 0; i < 200; i++) scratch.allocate(12, 8);
        int grown = scratch.chunkCount();
        System.out.println("  200 levels -> " + grown + " buffers, " + scratch.capacity() + "B live");
        if (grown <= 1) throw new AssertionError("deep book forced a grow");
        scratch.reset();
        int cap = scratch.capacity();
        for (int i = 0; i < 50; i++) scratch.allocate(12, 8);
        if (scratch.capacity() != cap) throw new AssertionError("steady-state tick is grow-free");
    }

    /** stats: {@link StatsArena} counters survive reset, so a long-running feed
     *  handler can size the arena from the observed peak. */
    static void statsSizing() {
        System.out.println("\n== stats: size the arena from real load ==");
        StatsArena scratch = new StatsArena(4096);
        for (int tick = 0; tick < 1_000; tick++) {
            for (int i = 0; i < 8; i++) scratch.allocate(12, 8);
            scratch.reset();
        }
        StatsArena.Stats s = scratch.stats();
        System.out.println("  " + s.allocations() + " allocs over 1000 ticks, peak "
            + s.peakBytes() + "B, wasted " + s.bytesWasted() + "B");
        if (s.allocations() != 8_000) throw new AssertionError("counters survive reset");
        if (s.peakBytes() <= 0) throw new AssertionError("peak recorded");
    }

    /** aligned: {@link AlignedArena} rounds the offset up to a cache line for a
     *  SIMD-style scan over a tick's prices. */
    static void alignedPriceScan() {
        System.out.println("\n== aligned: cache-line offset scratch for a price scan ==");
        AlignedArena scratch = new AlignedArena(1024);
        int off = scratch.allocAligned(64, 64);
        if ((off & 63) != 0) throw new AssertionError("64-byte aligned offset");
        byte[] region = scratch.bytes();
        int checksum = 0;
        for (int i = 0; i < 64; i++) {
            region[off + i] = (byte) i;
            checksum += i;
        }
        System.out.println("  64B aligned offset " + off + ", checksum " + checksum
            + ", used " + scratch.used() + "B");
        if (checksum != 2016) throw new AssertionError("checksum");
        scratch.reset();
        if (scratch.used() != 0) throw new AssertionError("reset rewinds");
    }
}
