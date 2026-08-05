package com.submillisecond.recipes.arena;

import com.submillisecond.recipes.arena.features.AlignedArena;
import com.submillisecond.recipes.arena.features.GrowableArena;
import com.submillisecond.recipes.arena.features.Slot;
import com.submillisecond.recipes.arena.features.StatsArena;
import com.submillisecond.recipes.arena.features.TypedArena;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    record Level(long priceTicks, int qty) {}

    @Test
    void quickstart() {
        // quickstart:begin
        BumpArena arena = new BumpArena(1024);
        int off = arena.allocate(4, 4);          // a 4-byte slot, 4-byte aligned
        arena.bytes()[off] = 42;
        assertEquals(42, arena.bytes()[off]);
        arena.reset();                           // rewind; the buffer is reused next round
        assertEquals(0, arena.used());
        // quickstart:end
    }

    @Test
    void perTickScratchResetsWithoutRealloc() {
        BumpArena scratch = new BumpArena(4096);
        ByteBuffer buf = ByteBuffer.wrap(scratch.bytes());
        int cap = scratch.capacity();
        int[] updates = {9998, 5, 0, 9997, 8, 0, 10002, 4, 1, 10003, 9, 1};

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
        assertEquals(10_000, (bestBid + bestAsk) / 2, "mid of the tick");
        assertEquals(0, bidQty - askQty, "balanced book");
        assertTrue(scratch.used() > 0);

        scratch.reset();
        assertEquals(0, scratch.used());
        assertEquals(cap, scratch.capacity(), "no reallocation across ticks");
    }

    @Test
    void typedSnapshotReadsBackAndRecycles() {
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
        assertEquals(4, book.len());
        assertEquals(26, resting);
        assertEquals(10_003, top);

        Slot cancelled = live.remove(live.size() - 1);
        int freedIndex = cancelled.index();
        book.free(cancelled);
        Slot replacement = book.alloc(new Level(10_004, 2));
        assertEquals(freedIndex, replacement.index(), "freed slot came back");
        assertEquals(1, book.reuseHits());
        assertEquals(4, book.len(), "cancel + replace is footprint-neutral");

        book.reset();
        assertTrue(book.isEmpty());
    }

    @Test
    void growableDeepTickThenGrowFreeSteadyState() {
        GrowableArena scratch = new GrowableArena(256);
        for (int i = 0; i < 200; i++) scratch.allocate(12, 8);
        assertTrue(scratch.chunkCount() > 1, "deep book forced a grow");
        scratch.reset();
        int cap = scratch.capacity();
        for (int i = 0; i < 50; i++) scratch.allocate(12, 8);
        assertEquals(cap, scratch.capacity(), "steady-state tick is grow-free");
    }

    @Test
    void statsCountersSurviveReset() {
        StatsArena scratch = new StatsArena(4096);
        for (int t = 0; t < 1_000; t++) {
            for (int i = 0; i < 8; i++) scratch.allocate(12, 8);
            scratch.reset();
        }
        StatsArena.Stats s = scratch.stats();
        assertEquals(8_000, s.allocations());
        assertTrue(s.peakBytes() > 0);
    }

    @Test
    void alignedScratchOffsetIsCacheLineAligned() {
        AlignedArena scratch = new AlignedArena(1024);
        int off = scratch.allocAligned(64, 64);
        assertEquals(0, off & 63);
        scratch.reset();
        assertEquals(0, scratch.used());
    }
}
