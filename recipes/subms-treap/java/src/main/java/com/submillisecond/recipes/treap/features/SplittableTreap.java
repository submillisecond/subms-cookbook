package com.submillisecond.recipes.treap.features;

import java.util.AbstractMap;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Sequence-builder treap with explicit {@link #split(Comparable)} and
 * {@link #merge(SplittableTreap, SplittableTreap)} operations.
 *
 * <p>Both run in {@code O(log N)} expected time under the standard
 * treap rotation invariant. The pair is the textbook implicit-treap
 * toolkit: build two halves separately and merge, or chop a range
 * out of a sorted stream via split.
 *
 * <p>{@code merge} requires every key in {@code left} strictly less
 * than every key in {@code right}. Violation throws
 * {@link IllegalArgumentException}.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_treap::SplittableTreap}.
 */
public final class SplittableTreap<K extends Comparable<K>, V> {

    private static final class Node<K, V> {
        K key;
        V value;
        long priority;
        Node<K, V> left, right;
        Node(K key, V value, long priority) {
            this.key = key;
            this.value = value;
            this.priority = priority;
        }
    }

    private Node<K, V> root;
    private int size;
    private long rngState;

    public SplittableTreap(long seed) {
        this(null, 0, seed | 1L);
    }

    private SplittableTreap(Node<K, V> root, int size, long rngState) {
        this.root = root;
        this.size = size;
        this.rngState = rngState;
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

    public List<Map.Entry<K, V>> collectInOrder() {
        List<Map.Entry<K, V>> out = new ArrayList<>(size);
        inOrder(root, out);
        return out;
    }

    /**
     * Split this treap into {@code (left, right)} where every key in
     * {@code left} is strictly less than {@code pivot} and every key
     * in {@code right} is greater-than-or-equal-to {@code pivot}.
     * After return, this treap is left empty.
     */
    public Split<K, V> split(K pivot) {
        SplitResult<K, V> sr = splitNode(root, pivot);
        root = null;
        size = 0;
        int loCount = count(sr.left);
        int hiCount = count(sr.right);
        SplittableTreap<K, V> lo = new SplittableTreap<>(sr.left, loCount, rngState);
        SplittableTreap<K, V> hi = new SplittableTreap<>(sr.right, hiCount, rngState + 1);
        return new Split<>(lo, hi);
    }

    /**
     * Merge {@code left} and {@code right} into a single treap. Both
     * inputs are consumed (drained). Every key in {@code left} must
     * be strictly less than every key in {@code right}.
     */
    public static <K extends Comparable<K>, V> SplittableTreap<K, V> merge(
            SplittableTreap<K, V> left, SplittableTreap<K, V> right) {
        K lMax = maxKey(left.root);
        K rMin = minKey(right.root);
        if (lMax != null && rMin != null && lMax.compareTo(rMin) >= 0) {
            throw new IllegalArgumentException(
                    "SplittableTreap.merge precondition: left max < right min required");
        }
        Node<K, V> merged = mergeNodes(left.root, right.root);
        int size = left.size + right.size;
        long rng = (left.rngState + right.rngState) | 1L;
        left.root = null;
        left.size = 0;
        right.root = null;
        right.size = 0;
        return new SplittableTreap<>(merged, size, rng);
    }

    /** Result of a split: the two resulting treaps. */
    public static final class Split<K extends Comparable<K>, V> {
        public final SplittableTreap<K, V> left;
        public final SplittableTreap<K, V> right;
        Split(SplittableTreap<K, V> left, SplittableTreap<K, V> right) {
            this.left = left;
            this.right = right;
        }
    }

    private long nextPriority() {
        rngState = rngState * 6364136223846793005L + 1442695040888963407L;
        // SplitMix64 finalizer - decorrelate priority from key. Mirrors
        // the base Treap fix; a bare LCG priority degenerates the tree.
        long z = rngState;
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        return z ^ (z >>> 31);
    }

    private static final class InsResult<K, V> {
        Node<K, V> root;
        V replaced;
        InsResult(Node<K, V> root, V replaced) {
            this.root = root;
            this.replaced = replaced;
        }
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
            if (Long.compareUnsigned(node.left.priority, node.priority) > 0) node = rotateRight(node);
            return new InsResult<>(node, r.replaced);
        }
        InsResult<K, V> r = ins(node.right, key, value, pri);
        node.right = r.root;
        if (Long.compareUnsigned(node.right.priority, node.priority) > 0) node = rotateLeft(node);
        return new InsResult<>(node, r.replaced);
    }

    private static final class SplitResult<K, V> {
        Node<K, V> left;
        Node<K, V> right;
        SplitResult(Node<K, V> left, Node<K, V> right) {
            this.left = left;
            this.right = right;
        }
    }

    private SplitResult<K, V> splitNode(Node<K, V> node, K pivot) {
        if (node == null) return new SplitResult<>(null, null);
        if (node.key.compareTo(pivot) < 0) {
            SplitResult<K, V> r = splitNode(node.right, pivot);
            node.right = r.left;
            return new SplitResult<>(node, r.right);
        }
        SplitResult<K, V> r = splitNode(node.left, pivot);
        node.left = r.right;
        return new SplitResult<>(r.left, node);
    }

    private static <K, V> Node<K, V> mergeNodes(Node<K, V> left, Node<K, V> right) {
        if (left == null) return right;
        if (right == null) return left;
        if (Long.compareUnsigned(left.priority, right.priority) > 0) {
            left.right = mergeNodes(left.right, right);
            return left;
        }
        right.left = mergeNodes(left, right.left);
        return right;
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

    private static <K, V> int count(Node<K, V> node) {
        if (node == null) return 0;
        return 1 + count(node.left) + count(node.right);
    }

    private static <K, V> K minKey(Node<K, V> node) {
        if (node == null) return null;
        while (node.left != null) node = node.left;
        return node.key;
    }

    private static <K, V> K maxKey(Node<K, V> node) {
        if (node == null) return null;
        while (node.right != null) node = node.right;
        return node.key;
    }

    private void inOrder(Node<K, V> n, List<Map.Entry<K, V>> out) {
        if (n == null) return;
        inOrder(n.left, out);
        out.add(new AbstractMap.SimpleImmutableEntry<>(n.key, n.value));
        inOrder(n.right, out);
    }
}
