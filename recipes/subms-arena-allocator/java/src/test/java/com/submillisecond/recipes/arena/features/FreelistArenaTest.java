package com.submillisecond.recipes.arena.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertSame;
import static org.junit.jupiter.api.Assertions.assertThrows;

final class FreelistArenaTest {

    static final class Node {
        int v;
    }

    @Test
    void freshAllocateUsesFactory() {
        FreelistArena<Node> a = new FreelistArena<>(4, Node::new);
        Node n = a.allocate();
        assertEquals(1, a.issued());
        assertEquals(0, a.reuseHits());
        assertEquals(0, n.v);
    }

    @Test
    void releaseAndReuse() {
        FreelistArena<Node> a = new FreelistArena<>(4, Node::new);
        Node n1 = a.allocate();
        n1.v = 7;
        a.release(n1);
        Node n2 = a.allocate();
        assertSame(n1, n2, "released object must be reused");
        assertEquals(1, a.reuseHits());
    }

    @Test
    void lifoReuseOrder() {
        FreelistArena<Node> a = new FreelistArena<>(8, Node::new);
        Node n1 = a.allocate();
        Node n2 = a.allocate();
        Node n3 = a.allocate();
        a.release(n1);
        a.release(n2);
        a.release(n3);
        assertSame(n3, a.allocate());
        assertSame(n2, a.allocate());
        assertSame(n1, a.allocate());
    }

    @Test
    void distinctIfNotReleased() {
        FreelistArena<Node> a = new FreelistArena<>(4, Node::new);
        Node n1 = a.allocate();
        Node n2 = a.allocate();
        assertNotSame(n1, n2);
    }

    @Test
    void capacityExhaustionWithoutRelease() {
        FreelistArena<Node> a = new FreelistArena<>(2, Node::new);
        a.allocate();
        a.allocate();
        assertThrows(IllegalStateException.class, a::allocate);
    }

    @Test
    void resetClearsFreelist() {
        FreelistArena<Node> a = new FreelistArena<>(4, Node::new);
        Node n = a.allocate();
        a.release(n);
        assertEquals(1, a.freelistLen());
        a.reset();
        assertEquals(0, a.freelistLen());
        assertEquals(0, a.issued());
        assertEquals(0, a.reuseHits());
    }

    @Test
    void freelistLenTracksBucket() {
        FreelistArena<Node> a = new FreelistArena<>(4, Node::new);
        Node n1 = a.allocate();
        Node n2 = a.allocate();
        assertEquals(0, a.freelistLen());
        a.release(n1);
        assertEquals(1, a.freelistLen());
        a.release(n2);
        assertEquals(2, a.freelistLen());
    }
}
