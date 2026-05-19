package com.submillisecond.recipes.treap;

import java.util.ArrayList;
import java.util.List;

/**
 * Treap: probabilistic balanced BST. Each node has a random priority; the
 * tree is BST on keys and max-heap on priorities. Expected height
 * {@code O(log n)} with uniform priorities.
 */
public final class Treap<K extends Comparable<K>, V> {

    private static final class Node<K, V> {
        K key;
        V value;
        long priority;
        Node<K, V> left, right;
        Node(K k, V v, long p) { this.key = k; this.value = v; this.priority = p; }
    }

    private Node<K, V> root;
    private int size;
    private long rngState;

    public Treap(long seed) {
        this.rngState = seed | 1L;
    }

    public int size() { return size; }
    public boolean isEmpty() { return size == 0; }

    public V insert(K key, V value) {
        long pri = nextPriority();
        InsResult<K, V> r = ins(root, key, value, pri);
        root = r.root;
        if (r.replaced == null) size++;
        return r.replaced;
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

    public V remove(K key) {
        RemResult<K, V> r = rem(root, key);
        root = r.root;
        if (r.removed != null) size--;
        return r.removed;
    }

    public List<K> collectInOrder() {
        List<K> out = new ArrayList<>(size);
        inOrder(root, out);
        return out;
    }

    private long nextPriority() {
        rngState = rngState * 6364136223846793005L + 1442695040888963407L;
        return rngState;
    }

    private static final class InsResult<K, V> {
        Node<K, V> root;
        V replaced;
        InsResult(Node<K, V> r, V rep) { this.root = r; this.replaced = rep; }
    }

    private InsResult<K, V> ins(Node<K, V> node, K key, V value, long pri) {
        if (node == null) return new InsResult<>(new Node<>(key, value, pri), null);
        int cmp = key.compareTo(node.key);
        if (cmp == 0) {
            V old = node.value;
            node.value = value;
            return new InsResult<>(node, old);
        }
        if (cmp < 0) {
            InsResult<K, V> r = ins(node.left, key, value, pri);
            node.left = r.root;
            if (node.left.priority > node.priority) node = rotateRight(node);
            return new InsResult<>(node, r.replaced);
        } else {
            InsResult<K, V> r = ins(node.right, key, value, pri);
            node.right = r.root;
            if (node.right.priority > node.priority) node = rotateLeft(node);
            return new InsResult<>(node, r.replaced);
        }
    }

    private static final class RemResult<K, V> {
        Node<K, V> root;
        V removed;
        RemResult(Node<K, V> r, V rem) { this.root = r; this.removed = rem; }
    }

    private RemResult<K, V> rem(Node<K, V> node, K key) {
        if (node == null) return new RemResult<>(null, null);
        int cmp = key.compareTo(node.key);
        if (cmp < 0) {
            RemResult<K, V> r = rem(node.left, key);
            node.left = r.root;
            return new RemResult<>(node, r.removed);
        }
        if (cmp > 0) {
            RemResult<K, V> r = rem(node.right, key);
            node.right = r.root;
            return new RemResult<>(node, r.removed);
        }
        V value = node.value;
        Node<K, V> merged = mergeSubtrees(node.left, node.right);
        return new RemResult<>(merged, value);
    }

    private Node<K, V> mergeSubtrees(Node<K, V> left, Node<K, V> right) {
        if (left == null) return right;
        if (right == null) return left;
        if (left.priority > right.priority) {
            left.right = mergeSubtrees(left.right, right);
            return left;
        } else {
            right.left = mergeSubtrees(left, right.left);
            return right;
        }
    }

    private Node<K, V> rotateRight(Node<K, V> node) {
        Node<K, V> l = node.left;
        node.left = l.right;
        l.right = node;
        return l;
    }

    private Node<K, V> rotateLeft(Node<K, V> node) {
        Node<K, V> r = node.right;
        node.right = r.left;
        r.left = node;
        return r;
    }

    private void inOrder(Node<K, V> n, List<K> out) {
        if (n == null) return;
        inOrder(n.left, out);
        out.add(n.key);
        inOrder(n.right, out);
    }
}
