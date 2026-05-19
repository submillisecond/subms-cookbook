package com.submillisecond.recipes.art;

import java.util.HashMap;
import java.util.Map;

/**
 * Adaptive Radix Tree over byte-string keys. Small nodes (<= 4 children)
 * use a 4-slot array scanned linearly; once a 5th child is needed, the node
 * grows to a HashMap-backed Full variant. Path compression is omitted.
 */
public final class Art<V> {

    private static final class Node<V> {
        V value;
        Object children = new SmallChildren<V>();
    }

    private static final class SmallChildren<V> {
        final byte[] keys = new byte[4];
        @SuppressWarnings("unchecked")
        final Node<V>[] children = (Node<V>[]) new Node[4];
        int count;
    }

    private final Node<V> root = new Node<>();
    private int size;

    public int size() { return size; }
    public boolean isEmpty() { return size == 0; }

    public V insert(byte[] key, V value) {
        Node<V> cur = root;
        for (byte b : key) {
            cur = getOrInsertChild(cur, b);
        }
        V prev = cur.value;
        cur.value = value;
        if (prev == null) size++;
        return prev;
    }

    public V get(byte[] key) {
        Node<V> cur = root;
        for (byte b : key) {
            cur = getChild(cur, b);
            if (cur == null) return null;
        }
        return cur.value;
    }

    @SuppressWarnings("unchecked")
    private Node<V> getChild(Node<V> n, byte b) {
        Object kids = n.children;
        if (kids instanceof SmallChildren) {
            SmallChildren<V> s = (SmallChildren<V>) kids;
            for (int i = 0; i < s.count; i++) {
                if (s.keys[i] == b) return s.children[i];
            }
            return null;
        }
        return ((Map<Byte, Node<V>>) kids).get(b);
    }

    @SuppressWarnings("unchecked")
    private Node<V> getOrInsertChild(Node<V> n, byte b) {
        Object kids = n.children;
        if (kids instanceof SmallChildren) {
            SmallChildren<V> s = (SmallChildren<V>) kids;
            for (int i = 0; i < s.count; i++) {
                if (s.keys[i] == b) return s.children[i];
            }
            if (s.count < 4) {
                Node<V> child = new Node<>();
                s.keys[s.count] = b;
                s.children[s.count] = child;
                s.count++;
                return child;
            }
            // Promote to Full.
            Map<Byte, Node<V>> map = new HashMap<>(8);
            for (int i = 0; i < 4; i++) map.put(s.keys[i], s.children[i]);
            Node<V> child = new Node<>();
            map.put(b, child);
            n.children = map;
            return child;
        }
        Map<Byte, Node<V>> map = (Map<Byte, Node<V>>) kids;
        Node<V> child = map.get(b);
        if (child != null) return child;
        child = new Node<>();
        map.put(b, child);
        return child;
    }
}
