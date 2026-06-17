package com.submillisecond.recipes.arena.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class TypedArenaTest {

    static final class Node {
        int v;
        Node() {}
    }

    @Test
    void allocateReturnsTypedInstance() {
        TypedArena<Node> a = new TypedArena<>(4, Node::new);
        Node n = a.allocate();
        n.v = 42;
        assertEquals(42, n.v);
    }

    @Test
    void fillsToCapacityThenRejects() {
        TypedArena<Node> a = new TypedArena<>(3, Node::new);
        a.allocate();
        a.allocate();
        a.allocate();
        assertEquals(3, a.len());
        assertThrows(IllegalStateException.class, a::allocate);
    }

    @Test
    void resetRecyclesInstances() {
        TypedArena<Node> a = new TypedArena<>(4, Node::new);
        Node n1 = a.allocate();
        Node n2 = a.allocate();
        a.reset();
        assertEquals(0, a.len());
        // After reset the same backing slot is handed out again.
        assertSame(n1, a.allocate(), "first slot recycled");
        assertSame(n2, a.allocate(), "second slot recycled");
    }

    @Test
    void distinctSlotsAreDistinctInstances() {
        TypedArena<Node> a = new TypedArena<>(4, Node::new);
        Node n1 = a.allocate();
        Node n2 = a.allocate();
        assertNotSame(n1, n2);
    }

    @Test
    void cacheLineSizedObjectAllocates() {
        // Java analogue of the Rust 64-byte alignment test: hold a
        // type whose footprint matches a cache line. We can't assert
        // absolute address alignment in portable Java, but we can
        // confirm the typed pool happily holds a non-trivial type.
        final class CacheLine {
            final long a, b, c, d, e, f, g, h;
            CacheLine() { a = b = c = d = e = f = g = h = 0L; }
        }
        TypedArena<CacheLine> a = new TypedArena<>(4, CacheLine::new);
        for (int i = 0; i < 4; i++) {
            assertNotNull(a.allocate());
        }
    }

    @Test
    void isEmptyReflectsLen() {
        TypedArena<Node> a = new TypedArena<>(2, Node::new);
        assertTrue(a.isEmpty());
        a.allocate();
        assertEquals(false, a.isEmpty());
        a.reset();
        assertTrue(a.isEmpty());
    }

    @Test
    void capacityLessThanOneRejected() {
        assertThrows(IllegalArgumentException.class, () -> new TypedArena<>(0, Node::new));
    }

    static <T> void assertNotNull(T t) {
        if (t == null) throw new AssertionError("expected non-null");
    }
}
