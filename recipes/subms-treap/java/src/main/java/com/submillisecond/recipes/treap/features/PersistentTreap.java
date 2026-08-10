package com.submillisecond.recipes.treap.features;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.AbstractMap;

/**
 * Persistent (copy-on-write) treap.
 *
 * <p>Every {@code insert} / {@code remove} returns a NEW
 * {@code PersistentTreap} without mutating the receiver. The
 * mutation path is rebuilt with new {@code Node} instances
 * ({@code O(log N)}); every other subtree is shared via the same
 * immutable {@code Node} reference. Old versions remain fully
 * queryable.
 *
 * <p>Java has no Rust-style {@code Arc}; the persistence guarantee
 * comes from {@code Node} being immutable (every field {@code final})
 * and from the mutation path always producing a fresh chain.
 * Concurrent reads on a held version are safe by construction.
 *
 * <p>Byte-equivalent to the Rust sibling {@code subms_treap::PersistentTreap}.
 */
public final class PersistentTreap<K extends Comparable<K>, V> {

    private static final class Node<K, V> {
        final K key;
        final V value;
        final long priority;
        final Node<K, V> left;
        final Node<K, V> right;

        Node(K key, V value, long priority, Node<K, V> left, Node<K, V> right) {
            this.key = key;
            this.value = value;
            this.priority = priority;
            this.left = left;
            this.right = right;
        }
    }

    private final Node<K, V> root;
    private final int size;
    private final long rngState;

    public PersistentTreap(long seed) {
        this(null, 0, seed | 1L);
    }

    private PersistentTreap(Node<K, V> root, int size, long rngState) {
        this.root = root;
        this.size = size;
        this.rngState = rngState;
    }

    public int size() { return size; }
    public boolean isEmpty() { return size == 0; }

    /** Return a NEW treap with {@code (key, value)} inserted (or its
     *  value replaced). {@code this} is left untouched. Shared
     *  subtrees keep the same {@code Node} references. */
    public PersistentTreap<K, V> insert(K key, V value) {
        long nextRng = rngState * 6364136223846793005L + 1442695040888963407L;
        // SplitMix64 finalizer on the priority (not the stored state) -
        // decorrelate priority from key so the tree keeps expected
        // O(log n) height. Mirrors the base Treap fix. The advanced LCG
        // state `nextRng` is what carries forward; only the priority that
        // shapes the heap is avalanched.
        long z = nextRng;
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        long priority = z ^ (z >>> 31);
        InsResult<K, V> r = ins(root, key, value, priority);
        int newSize = r.replaced ? size : size + 1;
        return new PersistentTreap<>(r.root, newSize, nextRng);
    }

    /** Return a NEW treap with {@code key} removed. If absent, the
     *  returned treap shares structure with {@code this}. */
    public PersistentTreap<K, V> remove(K key) {
        RemResult<K, V> r = rem(root, key);
        int newSize = r.removed ? size - 1 : size;
        return new PersistentTreap<>(r.root, newSize, rngState);
    }

    public V get(K key) {
        Node<K, V> cur = root;
        while (cur != null) {
            int cmp = key.compareTo(cur.key);
            if (cmp < 0) cur = cur.left;
            else if (cmp > 0) cur = cur.right;
            else return cur.value;
        }
        return null;
    }

    public List<Map.Entry<K, V>> collectInOrder() {
        List<Map.Entry<K, V>> out = new ArrayList<>(size);
        inOrder(root, out);
        return out;
    }

    private static final class InsResult<K, V> {
        final Node<K, V> root;
        final boolean replaced;
        InsResult(Node<K, V> root, boolean replaced) {
            this.root = root;
            this.replaced = replaced;
        }
    }

    private InsResult<K, V> ins(Node<K, V> node, K key, V value, long priority) {
        if (node == null) {
            return new InsResult<>(new Node<>(key, value, priority, null, null), false);
        }
        int cmp = key.compareTo(node.key);
        if (cmp == 0) {
            Node<K, V> replaced = new Node<>(key, value, node.priority, node.left, node.right);
            return new InsResult<>(replaced, true);
        }
        if (cmp < 0) {
            InsResult<K, V> r = ins(node.left, key, value, priority);
            Node<K, V> rebuilt = new Node<>(node.key, node.value, node.priority, r.root, node.right);
            Node<K, V> rooted = (Long.compareUnsigned(r.root.priority, node.priority) > 0) ? rotateRight(rebuilt) : rebuilt;
            return new InsResult<>(rooted, r.replaced);
        }
        InsResult<K, V> r = ins(node.right, key, value, priority);
        Node<K, V> rebuilt = new Node<>(node.key, node.value, node.priority, node.left, r.root);
        Node<K, V> rooted = (Long.compareUnsigned(r.root.priority, node.priority) > 0) ? rotateLeft(rebuilt) : rebuilt;
        return new InsResult<>(rooted, r.replaced);
    }

    private static final class RemResult<K, V> {
        final Node<K, V> root;
        final boolean removed;
        RemResult(Node<K, V> root, boolean removed) {
            this.root = root;
            this.removed = removed;
        }
    }

    private RemResult<K, V> rem(Node<K, V> node, K key) {
        if (node == null) return new RemResult<>(null, false);
        int cmp = key.compareTo(node.key);
        if (cmp < 0) {
            RemResult<K, V> r = rem(node.left, key);
            Node<K, V> rebuilt = new Node<>(node.key, node.value, node.priority, r.root, node.right);
            return new RemResult<>(rebuilt, r.removed);
        }
        if (cmp > 0) {
            RemResult<K, V> r = rem(node.right, key);
            Node<K, V> rebuilt = new Node<>(node.key, node.value, node.priority, node.left, r.root);
            return new RemResult<>(rebuilt, r.removed);
        }
        return new RemResult<>(mergeSubtrees(node.left, node.right), true);
    }

    private Node<K, V> mergeSubtrees(Node<K, V> left, Node<K, V> right) {
        if (left == null) return right;
        if (right == null) return left;
        if (Long.compareUnsigned(left.priority, right.priority) > 0) {
            return new Node<>(left.key, left.value, left.priority,
                    left.left, mergeSubtrees(left.right, right));
        }
        return new Node<>(right.key, right.value, right.priority,
                mergeSubtrees(left, right.left), right.right);
    }

    private Node<K, V> rotateRight(Node<K, V> node) {
        Node<K, V> l = node.left;
        Node<K, V> newRight = new Node<>(node.key, node.value, node.priority, l.right, node.right);
        return new Node<>(l.key, l.value, l.priority, l.left, newRight);
    }

    private Node<K, V> rotateLeft(Node<K, V> node) {
        Node<K, V> r = node.right;
        Node<K, V> newLeft = new Node<>(node.key, node.value, node.priority, node.left, r.left);
        return new Node<>(r.key, r.value, r.priority, newLeft, r.right);
    }

    private void inOrder(Node<K, V> n, List<Map.Entry<K, V>> out) {
        if (n == null) return;
        inOrder(n.left, out);
        out.add(new AbstractMap.SimpleImmutableEntry<>(n.key, n.value));
        inOrder(n.right, out);
    }
}
