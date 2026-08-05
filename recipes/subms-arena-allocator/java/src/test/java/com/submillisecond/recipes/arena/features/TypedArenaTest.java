package com.submillisecond.recipes.arena.features;

import java.util.Optional;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Mirrors {@code rust/src/features/typed_tests.rs} test for test. */
final class TypedArenaTest {

    static final class Node {
        final int v;

        Node(int v) {
            this.v = v;
        }
    }

    @Test
    void allocReturnsHandleThatReadsBack() {
        TypedArena<Node> a = new TypedArena<>(16);
        Slot s = a.alloc(new Node(42));
        assertEquals(42, a.get(s).v);
        assertEquals(1, a.len());
    }

    @Test
    void setOverwritesInPlace() {
        TypedArena<Node> a = new TypedArena<>(16);
        Slot s = a.alloc(new Node(42));
        a.set(s, new Node(99));
        assertEquals(99, a.get(s).v);
        assertEquals(1, a.len(), "set is not an allocation");
    }

    @Test
    void slotIndexFollowsAllocationOrder() {
        TypedArena<Node> a = new TypedArena<>(4);
        assertEquals(0, a.alloc(new Node(10)).index());
        assertEquals(1, a.alloc(new Node(20)).index());
    }

    @Test
    void fillsToCapacityThenTryAllocIsEmpty() {
        TypedArena<Node> a = new TypedArena<>(4);
        for (int i = 0; i < 4; i++) {
            a.alloc(new Node(i));
        }
        assertEquals(Optional.empty(), a.tryAlloc(new Node(99)), "full arena refuses");
        assertEquals(4, a.len());
    }

    @Test
    void allocThrowsWhenFull() {
        TypedArena<Node> a = new TypedArena<>(2);
        a.alloc(new Node(1));
        a.alloc(new Node(2));
        IllegalStateException e =
            assertThrows(IllegalStateException.class, () -> a.alloc(new Node(3)));
        assertTrue(e.getMessage().startsWith("TypedArena full"), e.getMessage());
    }

    @Test
    void freedSlotIsReused() {
        TypedArena<Node> a = new TypedArena<>(8);
        Slot s = a.alloc(new Node(1));
        int idx = s.index();
        a.free(s);
        assertEquals(0, a.len(), "freeing drops the live count");
        Slot reused = a.alloc(new Node(2));
        assertEquals(idx, reused.index(), "freed slot comes back");
        assertEquals(2, a.get(reused).v);
        assertEquals(1, a.reuseHits());
    }

    @Test
    void reuseIsLifo() {
        TypedArena<Node> a = new TypedArena<>(8);
        Slot s0 = a.alloc(new Node(0));
        Slot s1 = a.alloc(new Node(1));
        Slot s2 = a.alloc(new Node(2));
        a.free(s0);
        a.free(s1);
        a.free(s2);
        assertEquals(2, a.alloc(new Node(10)).index(), "last freed is first reused");
        assertEquals(1, a.alloc(new Node(11)).index());
        assertEquals(0, a.alloc(new Node(12)).index());
        assertEquals(3, a.reuseHits());
    }

    @Test
    void reuseHitsCountsOnlyRecycledAllocs() {
        TypedArena<Node> a = new TypedArena<>(8);
        Slot s = a.alloc(new Node(1));
        a.alloc(new Node(2));
        assertEquals(0, a.reuseHits(), "fresh slots are not reuse");
        a.free(s);
        a.alloc(new Node(3));
        assertEquals(1, a.reuseHits());
    }

    @Test
    void reuseKeepsTheHighWaterMarkFlat() {
        // The reason the free stack exists: churn inside one arena lifetime
        // must not consume fresh slots.
        TypedArena<Node> a = new TypedArena<>(2);
        for (int i = 0; i < 1_000; i++) {
            Slot s = a.alloc(new Node(i));
            assertEquals(i, a.get(s).v);
            a.free(s);
        }
        assertEquals(999, a.reuseHits());
        assertTrue(a.isEmpty());
        a.alloc(new Node(0));
        a.alloc(new Node(1));
        assertEquals(2, a.len(), "both slots still available after the churn");
    }

    @Test
    void readAfterFreeSeesThePreviousOccupant() {
        // Documented behaviour, and a caller bug rather than undefined
        // behaviour: free does not clear the storage, and Java cannot consume
        // the handle the way Rust does.
        TypedArena<Node> a = new TypedArena<>(4);
        Node held = new Node(7);
        Slot s = a.alloc(held);
        a.free(s);
        assertSame(held, a.get(s), "the freed slot still holds the old value");
        Slot reused = a.alloc(new Node(9));
        assertEquals(9, a.get(reused).v, "reuse overwrites it");
    }

    @Test
    void resetClearsSlotsFreeStackAndCounter() {
        TypedArena<Node> a = new TypedArena<>(4);
        Slot s = a.alloc(new Node(1));
        a.alloc(new Node(2));
        a.free(s);
        a.alloc(new Node(3));
        assertEquals(1, a.reuseHits());
        a.reset();
        assertEquals(0, a.len());
        assertTrue(a.isEmpty());
        assertEquals(0, a.reuseHits());
        assertEquals(4, a.capacity(), "capacity survives reset");
    }

    @Test
    void resetThenAllocStartsFromSlotZero() {
        TypedArena<Node> a = new TypedArena<>(4);
        a.alloc(new Node(1));
        a.alloc(new Node(2));
        a.reset();
        assertEquals(0, a.alloc(new Node(3)).index());
    }

    @Test
    void capacityAndIsEmptyReportState() {
        TypedArena<Node> a = new TypedArena<>(8);
        assertEquals(8, a.capacity());
        assertTrue(a.isEmpty());
        Slot s = a.alloc(new Node(1));
        assertFalse(a.isEmpty());
        a.free(s);
        assertTrue(a.isEmpty());
    }

    @Test
    void capacityIsFlooredAtOne() {
        TypedArena<Node> a = new TypedArena<>(0);
        assertEquals(1, a.capacity());
        Slot s = a.alloc(new Node(1));
        assertEquals(Optional.empty(), a.tryAlloc(new Node(2)));
        a.free(s);
        assertTrue(a.tryAlloc(new Node(2)).isPresent(), "the single slot is reusable");
    }

    @Test
    void holdsACacheLineSizedType() {
        record CacheLine(long a, long b, long c, long d, long e, long f, long g, long h) {}
        TypedArena<CacheLine> a = new TypedArena<>(4);
        CacheLine line = new CacheLine(7, 7, 7, 7, 7, 7, 7, 7);
        Slot s = a.alloc(line);
        assertEquals(line, a.get(s));
        assertEquals(1, a.len());
    }

    @Test
    void slotIsPrintableAndComparable() {
        TypedArena<Node> a = new TypedArena<>(2);
        Slot s0 = a.alloc(new Node(1));
        Slot s1 = a.alloc(new Node(2));
        assertNotEquals(s0, s1);
        assertEquals(s0, new TypedArena<Node>(2).alloc(new Node(3)),
            "a handle is its index, not its arena");
        assertEquals(s0.hashCode(), s0.index());
        assertNotEquals(s0, "Slot(0)");
        assertEquals("Slot(0)", s0.toString());
    }
}
