package com.submillisecond.recipes.blockcache.features;

import java.util.HashMap;
import java.util.Map;

/**
 * Adaptive Replacement Cache (Megiddo + Modha, 2003).
 *
 * <p>Four lists, total resident budget {@code c}:
 *
 * <ul>
 *   <li>T1 - recently-seen-once entries (LRU at the back).</li>
 *   <li>T2 - recently-seen-more-than-once entries (LRU at the back).</li>
 *   <li>B1 - ghost list of keys recently evicted from T1.</li>
 *   <li>B2 - ghost list of keys recently evicted from T2.</li>
 * </ul>
 *
 * <p>Adapts the T1/T2 split based on which ghost list gets a hit:
 * a B1 hit grows {@code p} (recency); a B2 hit shrinks {@code p}
 * (frequency). Scan-resistant: a one-shot scan only lifts entries into
 * T1, then evicts them to B1, leaving T2 untouched.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_block_cache::features::arc::ArcCache}.
 */
public final class ArcCache<K, V> {

    private enum List { T1, T2, B1, B2 }

    private static final class Node<K, V> {
        K key;
        V value;
        int prev = -1;
        int next = -1;
        List list;
        Node(K k, V v, List l) { this.key = k; this.value = v; this.list = l; }
    }

    public record Evicted<K, V>(K key, V value) {}

    private final int c;
    private int p;
    private final java.util.ArrayList<Node<K, V>> nodes = new java.util.ArrayList<>();
    private final java.util.ArrayDeque<Integer> free = new java.util.ArrayDeque<>();
    private final Map<K, Integer> index = new HashMap<>();
    private int t1Head = -1, t1Tail = -1, t1Len;
    private int t2Head = -1, t2Tail = -1, t2Len;
    private int b1Head = -1, b1Tail = -1, b1Len;
    private int b2Head = -1, b2Tail = -1, b2Len;

    public ArcCache(int c) {
        this.c = Math.max(1, c);
    }

    public int capacity() { return c; }
    public int size() { return t1Len + t2Len; }
    public boolean isEmpty() { return size() == 0; }
    public int p() { return p; }
    public int t1Len() { return t1Len; }
    public int t2Len() { return t2Len; }
    public int b1Len() { return b1Len; }
    public int b2Len() { return b2Len; }

    /** Get + promote. T1 hit -> T2; T2 hit -> T2 MRU. Ghost-list keys
     *  return {@code null} (not resident). */
    public V get(K key) {
        Integer boxed = index.get(key);
        if (boxed == null) return null;
        int id = boxed;
        Node<K, V> n = nodes.get(id);
        switch (n.list) {
            case T1 -> {
                unlink(id);
                t1Len--;
                n.list = List.T2;
                pushFrontT2(id);
                t2Len++;
            }
            case T2 -> {
                unlink(id);
                t2Len--;
                pushFrontT2(id);
                t2Len++;
            }
            case B1, B2 -> { return null; }
        }
        return nodes.get(id).value;
    }

    /** Insert or update. Returns the evicted entry if eviction
     *  happened, else {@code null}. */
    public Evicted<K, V> put(K key, V value) {
        Integer existing = index.get(key);
        if (existing != null) {
            int id = existing;
            Node<K, V> n = nodes.get(id);
            switch (n.list) {
                case T1 -> {
                    unlink(id);
                    t1Len--;
                    n.value = value;
                    n.list = List.T2;
                    pushFrontT2(id);
                    t2Len++;
                    return null;
                }
                case T2 -> {
                    unlink(id);
                    n.value = value;
                    pushFrontT2(id);
                    return null;
                }
                case B1 -> {
                    int delta = Math.max(1, Math.max(1, b2Len) / Math.max(1, b1Len));
                    p = Math.min(c, p + delta);
                    Evicted<K, V> ev = replace(false);
                    unlink(id);
                    b1Len--;
                    n.value = value;
                    n.list = List.T2;
                    pushFrontT2(id);
                    t2Len++;
                    return ev;
                }
                case B2 -> {
                    int delta = Math.max(1, Math.max(1, b1Len) / Math.max(1, b2Len));
                    p = Math.max(0, p - delta);
                    Evicted<K, V> ev = replace(true);
                    unlink(id);
                    b2Len--;
                    n.value = value;
                    n.list = List.T2;
                    pushFrontT2(id);
                    t2Len++;
                    return ev;
                }
            }
            return null;
        }

        // Case IV: brand-new key.
        int l1 = t1Len + b1Len;
        int l2 = t2Len + b2Len;
        Evicted<K, V> evicted = null;
        if (l1 == c) {
            if (t1Len < c) {
                K victim = popLruB1();
                if (victim != null) index.remove(victim);
                evicted = replace(false);
            } else {
                int id = t1Tail;
                unlink(id);
                t1Len--;
                Node<K, V> n = nodes.set(id, null);
                index.remove(n.key);
                free.push(id);
                evicted = new Evicted<>(n.key, n.value);
            }
        } else if (l1 + l2 >= c) {
            if (l1 + l2 == 2 * c) {
                K victim = popLruB2();
                if (victim != null) index.remove(victim);
            }
            evicted = replace(false);
        }

        int id = alloc(new Node<>(key, value, List.T1));
        index.put(key, id);
        pushFrontT1(id);
        t1Len++;
        return evicted;
    }

    private Evicted<K, V> replace(boolean b2Hit) {
        boolean forceT1 = b2Hit && t1Len == p;
        if (t1Len > 0 && (t1Len > p || forceT1)) {
            int id = t1Tail;
            unlink(id);
            t1Len--;
            Node<K, V> n = nodes.get(id);
            V val = n.value;
            n.value = null;
            n.list = List.B1;
            pushFrontB1(id);
            b1Len++;
            return new Evicted<>(n.key, val);
        } else if (t2Len > 0) {
            int id = t2Tail;
            unlink(id);
            t2Len--;
            Node<K, V> n = nodes.get(id);
            V val = n.value;
            n.value = null;
            n.list = List.B2;
            pushFrontB2(id);
            b2Len++;
            return new Evicted<>(n.key, val);
        }
        return null;
    }

    private int alloc(Node<K, V> n) {
        if (!free.isEmpty()) {
            int id = free.pop();
            nodes.set(id, n);
            return id;
        }
        int id = nodes.size();
        nodes.add(n);
        return id;
    }

    private K popLruB1() {
        if (b1Tail == -1) return null;
        int id = b1Tail;
        unlink(id);
        b1Len--;
        Node<K, V> n = nodes.set(id, null);
        free.push(id);
        return n.key;
    }
    private K popLruB2() {
        if (b2Tail == -1) return null;
        int id = b2Tail;
        unlink(id);
        b2Len--;
        Node<K, V> n = nodes.set(id, null);
        free.push(id);
        return n.key;
    }

    private void unlink(int id) {
        Node<K, V> n = nodes.get(id);
        int prev = n.prev, next = n.next;
        List list = n.list;
        if (prev != -1) nodes.get(prev).next = next;
        if (next != -1) nodes.get(next).prev = prev;
        n.prev = -1;
        n.next = -1;
        switch (list) {
            case T1 -> {
                if (t1Head == id) t1Head = next;
                if (t1Tail == id) t1Tail = prev;
            }
            case T2 -> {
                if (t2Head == id) t2Head = next;
                if (t2Tail == id) t2Tail = prev;
            }
            case B1 -> {
                if (b1Head == id) b1Head = next;
                if (b1Tail == id) b1Tail = prev;
            }
            case B2 -> {
                if (b2Head == id) b2Head = next;
                if (b2Tail == id) b2Tail = prev;
            }
        }
    }

    private void pushFrontT1(int id) {
        int old = t1Head;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        t1Head = id;
        if (t1Tail == -1) t1Tail = id;
    }
    private void pushFrontT2(int id) {
        int old = t2Head;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        t2Head = id;
        if (t2Tail == -1) t2Tail = id;
    }
    private void pushFrontB1(int id) {
        int old = b1Head;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        b1Head = id;
        if (b1Tail == -1) b1Tail = id;
    }
    private void pushFrontB2(int id) {
        int old = b2Head;
        Node<K, V> n = nodes.get(id);
        n.next = old;
        n.prev = -1;
        if (old != -1) nodes.get(old).prev = id;
        b2Head = id;
        if (b2Tail == -1) b2Tail = id;
    }
}
