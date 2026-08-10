package com.submillisecond.recipes.treap;

import java.util.AbstractMap;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Deque;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.NoSuchElementException;
import java.util.function.UnaryOperator;

/**
 * Treap: probabilistic balanced BST. Each node has a random priority; the
 * tree is BST on keys and max-heap on priorities. Expected height
 * {@code O(log n)} with uniform priorities.
 */
public final class Treap<K extends Comparable<K>, V> implements Iterable<Map.Entry<K, V>> {

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

    /**
     * Seed the priority stream from the platform's entropy source rather than
     * from a constant.
     *
     * <p>The normal constructor takes an explicit seed because a reproducible
     * tree shape is what makes a benchmark and a bug report mean anything. That
     * same property is a liability when an attacker can both choose the keys
     * and observe the latency: the priority sequence is then known, and a
     * chosen key order can force the spine the randomized bound rules out.
     * Reach for this when the key stream is untrusted, and accept that two runs
     * no longer produce the same tree.
     */
    public static <K extends Comparable<K>, V> Treap<K, V> withRandomSeed() {
        return new Treap<>(new java.security.SecureRandom().nextLong());
    }

    /**
     * Build from already-sorted input in {@code O(n)}, skipping the n rotating
     * inserts a naive rebuild would pay. Keys must be strictly ascending;
     * duplicates are rejected rather than collapsed, because silently dropping
     * one of two entries is the wrong answer for every workload that reaches
     * for this. Pairs with {@link #collectEntriesInOrder()} as a snapshot /
     * restore round trip.
     *
     * @throws IllegalArgumentException if the input is not strictly ascending
     */
    public static <K extends Comparable<K>, V> Treap<K, V> fromSorted(
            long seed, List<Map.Entry<K, V>> items) {
        Treap<K, V> t = new Treap<>(seed);
        // Right spine of the tree built so far, priorities descending from the
        // root. Every new key exceeds everything already placed, so it can only
        // enter along that spine - which is the Cartesian-tree construction.
        Deque<Node<K, V>> spine = new ArrayDeque<>();
        for (int index = 0; index < items.size(); index++) {
            Map.Entry<K, V> item = items.get(index);
            if (!spine.isEmpty() && spine.peek().key.compareTo(item.getKey()) >= 0) {
                throw new IllegalArgumentException(
                        "fromSorted input not strictly ascending at index " + index);
            }
            Node<K, V> node = new Node<>(item.getKey(), item.getValue(), t.nextPriority());
            Node<K, V> demoted = null;
            while (!spine.isEmpty() && outranks(node.priority, spine.peek().priority)) {
                demoted = spine.pop();
            }
            node.left = demoted;
            if (spine.isEmpty()) {
                t.root = node;
            } else {
                spine.peek().right = node;
            }
            spine.push(node);
            t.size++;
        }
        return t;
    }

    public int size() { return size; }
    public boolean isEmpty() { return size == 0; }

    /**
     * Longest root-to-leaf path in edges; 0 for an empty or single-node tree.
     * The randomized-priority bound puts this near {@code 3 * ln(n)} in
     * expectation, so it is the cheapest way to see whether the priority
     * stream is doing its job on real keys.
     */
    public int height() {
        int best = 0;
        Deque<Node<K, V>> nodes = new ArrayDeque<>();
        Deque<Integer> depths = new ArrayDeque<>();
        if (root != null) { nodes.push(root); depths.push(0); }
        while (!nodes.isEmpty()) {
            Node<K, V> n = nodes.pop();
            int d = depths.pop();
            if (d > best) best = d;
            if (n.left != null) { nodes.push(n.left); depths.push(d + 1); }
            if (n.right != null) { nodes.push(n.right); depths.push(d + 1); }
        }
        return best;
    }

    /** Drop every entry and reset to empty. */
    public void clear() {
        root = null;
        size = 0;
    }

    public V insert(K key, V value) {
        long pri = nextPriority();
        InsResult<K, V> r = ins(root, key, value, pri);
        root = r.root;
        if (r.replaced == null) size++;
        return r.replaced;
    }

    public V get(K key) {
        Node<K, V> n = find(key);
        return n == null ? null : n.value;
    }

    public boolean containsKey(K key) {
        return find(key) != null;
    }

    /**
     * Apply {@code fn} to a resting value in place. The amend path for a price
     * level: no re-descent through {@link #insert}, no priority redraw, no
     * rotation. Returns the new value, or null when the key is absent. The
     * Java counterpart of the Rust port's {@code get_mut}.
     */
    public V compute(K key, UnaryOperator<V> fn) {
        Node<K, V> n = find(key);
        if (n == null) return null;
        n.value = fn.apply(n.value);
        return n.value;
    }

    public V remove(K key) {
        RemResult<K, V> r = rem(root, key);
        root = r.root;
        if (r.removed != null) size--;
        return r.removed;
    }

    /** Smallest key and its value, or null when empty. */
    public Map.Entry<K, V> first() {
        Node<K, V> n = spineEnd(false);
        return n == null ? null : entry(n);
    }

    /** Largest key and its value, or null when empty. */
    public Map.Entry<K, V> last() {
        Node<K, V> n = spineEnd(true);
        return n == null ? null : entry(n);
    }

    /** Greatest key {@code <= key}, or null. */
    public Map.Entry<K, V> floor(K key) {
        Node<K, V> n = searchLe(key, false);
        return n == null ? null : entry(n);
    }

    /** Least key {@code >= key}, or null. */
    public Map.Entry<K, V> ceiling(K key) {
        Node<K, V> n = searchGe(key, false);
        return n == null ? null : entry(n);
    }

    /** Greatest key strictly {@code < key}, or null. */
    public Map.Entry<K, V> predecessor(K key) {
        Node<K, V> n = searchLe(key, true);
        return n == null ? null : entry(n);
    }

    /** Least key strictly {@code > key}, or null. */
    public Map.Entry<K, V> successor(K key) {
        Node<K, V> n = searchGe(key, true);
        return n == null ? null : entry(n);
    }

    /** Remove and return the smallest entry, or null when empty. The
     *  top-of-book sweep. */
    public Map.Entry<K, V> popFirst() {
        return popExtreme(false);
    }

    /** Remove and return the largest entry, or null when empty. */
    public Map.Entry<K, V> popLast() {
        return popExtreme(true);
    }

    /**
     * Cut the treap at {@code pivot}, keeping everything below it and
     * returning everything at or above it.
     *
     * <p>One descent, expected {@code O(log n)}, no rebalancing pass. This is
     * the treap's distinguishing operation against a red-black tree, which has
     * no cheap equivalent. The object-per-node layout hands the detached
     * subtree over by reference, so unlike the Rust port's arena there is no
     * relocation cost.
     */
    public Treap<K, V> splitOff(K pivot) {
        SplitResult<K, V> parts = splitNode(root, pivot);
        root = parts.lo;
        Treap<K, V> upper = new Treap<>(rngState ^ 0x9e3779b97f4a7c15L);
        upper.root = parts.hi;
        upper.size = count(parts.hi);
        size -= upper.size;
        return upper;
    }

    /**
     * Splice {@code other} onto the end of this treap. Every key here must be
     * strictly below every key in {@code other}. Expected {@code O(log n)}.
     *
     * @throws IllegalArgumentException if the two key ranges overlap; both
     *         treaps are left untouched
     */
    public void join(Treap<K, V> other) {
        Map.Entry<K, V> mine = last();
        Map.Entry<K, V> theirs = other.first();
        if (mine != null && theirs != null && mine.getKey().compareTo(theirs.getKey()) >= 0) {
            throw new IllegalArgumentException(
                    "join requires every key on the left below every key on the right");
        }
        root = mergeSubtrees(root, other.root);
        size += other.size;
        other.root = null;
        other.size = 0;
    }

    /** Ascending in-order iteration. Lazy: the only allocation is the
     *  traversal stack, sized to the tree's height. */
    @Override
    public Iterator<Map.Entry<K, V>> iterator() {
        return new SpineIterator(false);
    }

    /**
     * Lazy ascending iteration over the entries between two bounds.
     * {@code from} / {@code to} may be null for unbounded ends; the booleans
     * pick inclusive vs exclusive. Descends to the lower bound in expected
     * {@code O(log n)} then walks only the window, and unlike
     * {@link #collectRange} it materialises nothing. Part of the default path:
     * an ordered index that cannot answer "everything between these two keys"
     * is a hash map with extra steps.
     */
    public Iterable<Map.Entry<K, V>> range(K from, boolean fromInclusive, K to, boolean toInclusive) {
        return () -> new RangeIterator(from, fromInclusive, to, toInclusive);
    }

    /** Descending in-order iteration. A bid ladder is read best price first,
     *  which is the reverse of the stored order. */
    public Iterator<Map.Entry<K, V>> descendingIterator() {
        return new SpineIterator(true);
    }

    public List<K> collectInOrder() {
        List<K> out = new ArrayList<>(size);
        inOrder(root, out);
        return out;
    }

    /** Sorted (key, value) pairs. Used by the {@code features.*}
     *  sub-package classes; sub-packages don't share Java's
     *  package-private access, so the accessor is public. */
    public List<Map.Entry<K, V>> collectEntriesInOrder() {
        List<Map.Entry<K, V>> out = new ArrayList<>(size);
        entriesInOrder(root, out);
        return out;
    }

    @Override
    public String toString() {
        StringBuilder sb = new StringBuilder("{");
        boolean firstEntry = true;
        for (Map.Entry<K, V> e : this) {
            if (!firstEntry) sb.append(", ");
            sb.append(e.getKey()).append(": ").append(e.getValue());
            firstEntry = false;
        }
        return sb.append('}').toString();
    }

    private void entriesInOrder(Node<K, V> n, List<Map.Entry<K, V>> out) {
        if (n == null) return;
        entriesInOrder(n.left, out);
        out.add(entry(n));
        entriesInOrder(n.right, out);
    }

    /**
     * Collect the {@code (key, value)} entries whose key falls within the
     * bounds, ascending. {@code from}/{@code to} may be null for unbounded
     * ends; the booleans pick inclusive vs exclusive. Descends to the lower
     * bound in {@code O(log n)} then walks only the window, so the cost is
     * {@code O(log n + window)} - not the {@code O(n)} of collecting the whole
     * tree and filtering. The returned list is an independent copy, stable
     * under later mutation of the source. Used by the range-query feature;
     * public because the sub-package cannot see the private Node type.
     */
    public List<Map.Entry<K, V>> collectRange(K from, boolean fromInclusive, K to, boolean toInclusive) {
        List<Map.Entry<K, V>> out = new ArrayList<>();
        ArrayDeque<Node<K, V>> stack = new ArrayDeque<>();
        // Descend to the smallest key >= from (or > from when exclusive),
        // pushing every candidate so the stack top is the in-order window start.
        Node<K, V> cur = root;
        while (cur != null) {
            boolean afterLow;
            if (from == null) {
                afterLow = true;
            } else {
                int c = cur.key.compareTo(from);
                afterLow = fromInclusive ? c >= 0 : c > 0;
            }
            if (afterLow) {
                stack.push(cur);
                cur = cur.left;
            } else {
                cur = cur.right;
            }
        }
        while (!stack.isEmpty()) {
            Node<K, V> n = stack.pop();
            if (to != null) {
                int c = n.key.compareTo(to);
                if (toInclusive ? c > 0 : c >= 0) break;   // past the upper bound
            }
            out.add(entry(n));
            // Push the right subtree's left spine - the in-order successors.
            Node<K, V> r = n.right;
            while (r != null) {
                stack.push(r);
                r = r.left;
            }
        }
        return out;
    }

    private static <K, V> Map.Entry<K, V> entry(Node<K, V> n) {
        return new AbstractMap.SimpleImmutableEntry<>(n.key, n.value);
    }

    private Node<K, V> find(K key) {
        Node<K, V> cur = root;
        while (cur != null) {
            int cmp = key.compareTo(cur.key);
            if (cmp < 0) cur = cur.left;
            else if (cmp > 0) cur = cur.right;
            else return cur;
        }
        return null;
    }

    private Node<K, V> spineEnd(boolean rightmost) {
        Node<K, V> cur = root;
        if (cur == null) return null;
        while (true) {
            Node<K, V> next = rightmost ? cur.right : cur.left;
            if (next == null) return cur;
            cur = next;
        }
    }

    private Node<K, V> searchLe(K key, boolean strict) {
        Node<K, V> cur = root, best = null;
        while (cur != null) {
            int cmp = cur.key.compareTo(key);
            if (strict ? cmp < 0 : cmp <= 0) {
                best = cur;
                cur = cur.right;
            } else {
                cur = cur.left;
            }
        }
        return best;
    }

    private Node<K, V> searchGe(K key, boolean strict) {
        Node<K, V> cur = root, best = null;
        while (cur != null) {
            int cmp = cur.key.compareTo(key);
            if (strict ? cmp > 0 : cmp >= 0) {
                best = cur;
                cur = cur.left;
            } else {
                cur = cur.right;
            }
        }
        return best;
    }

    private Map.Entry<K, V> popExtreme(boolean rightmost) {
        Node<K, V> target = spineEnd(rightmost);
        if (target == null) return null;
        Map.Entry<K, V> out = entry(target);
        root = detachExtreme(root, rightmost);
        size--;
        return out;
    }

    private Node<K, V> detachExtreme(Node<K, V> node, boolean rightmost) {
        Node<K, V> next = rightmost ? node.right : node.left;
        if (next == null) return rightmost ? node.left : node.right;
        if (rightmost) node.right = detachExtreme(next, true);
        else node.left = detachExtreme(next, false);
        return node;
    }

    private static final class SplitResult<K, V> {
        final Node<K, V> lo, hi;
        SplitResult(Node<K, V> lo, Node<K, V> hi) { this.lo = lo; this.hi = hi; }
    }

    /** Partition the subtree into keys below the pivot and keys at or above it.
     *  No rebalancing: neither half ever gains an ancestor it did not have. */
    private SplitResult<K, V> splitNode(Node<K, V> node, K pivot) {
        if (node == null) return new SplitResult<>(null, null);
        if (node.key.compareTo(pivot) < 0) {
            SplitResult<K, V> r = splitNode(node.right, pivot);
            node.right = r.lo;
            return new SplitResult<>(node, r.hi);
        }
        SplitResult<K, V> r = splitNode(node.left, pivot);
        node.left = r.hi;
        return new SplitResult<>(r.lo, node);
    }

    private static <K, V> int count(Node<K, V> node) {
        if (node == null) return 0;
        int n = 0;
        Deque<Node<K, V>> stack = new ArrayDeque<>();
        stack.push(node);
        while (!stack.isEmpty()) {
            Node<K, V> cur = stack.pop();
            n++;
            if (cur.left != null) stack.push(cur.left);
            if (cur.right != null) stack.push(cur.right);
        }
        return n;
    }

    private final class RangeIterator implements Iterator<Map.Entry<K, V>> {
        private final ArrayDeque<Node<K, V>> stack = new ArrayDeque<>();
        private final K to;
        private final boolean toInclusive;

        RangeIterator(K from, boolean fromInclusive, K to, boolean toInclusive) {
            this.to = to;
            this.toInclusive = toInclusive;
            // Descend to the smallest key at or after the lower bound, pushing
            // every candidate so the stack top is the window start.
            Node<K, V> cur = root;
            while (cur != null) {
                boolean afterLow = from == null;
                if (!afterLow) {
                    int c = cur.key.compareTo(from);
                    afterLow = fromInclusive ? c >= 0 : c > 0;
                }
                if (afterLow) {
                    stack.push(cur);
                    cur = cur.left;
                } else {
                    cur = cur.right;
                }
            }
            dropPastUpperBound();
        }

        private void dropPastUpperBound() {
            if (to == null || stack.isEmpty()) return;
            int c = stack.peek().key.compareTo(to);
            if (toInclusive ? c > 0 : c >= 0) stack.clear();
        }

        @Override
        public boolean hasNext() {
            return !stack.isEmpty();
        }

        @Override
        public Map.Entry<K, V> next() {
            if (stack.isEmpty()) throw new NoSuchElementException();
            Node<K, V> n = stack.pop();
            Node<K, V> r = n.right;
            while (r != null) {
                stack.push(r);
                r = r.left;
            }
            dropPastUpperBound();
            return entry(n);
        }
    }

    private final class SpineIterator implements Iterator<Map.Entry<K, V>> {
        private final ArrayDeque<Node<K, V>> stack = new ArrayDeque<>();
        private final boolean descending;

        SpineIterator(boolean descending) {
            this.descending = descending;
            push(root);
        }

        private void push(Node<K, V> n) {
            while (n != null) {
                stack.push(n);
                n = descending ? n.right : n.left;
            }
        }

        @Override
        public boolean hasNext() {
            return !stack.isEmpty();
        }

        @Override
        public Map.Entry<K, V> next() {
            if (stack.isEmpty()) throw new NoSuchElementException();
            Node<K, V> n = stack.pop();
            push(descending ? n.left : n.right);
            return entry(n);
        }
    }

    /** Priorities are unsigned 64-bit draws. Java's {@code >} on a long is
     *  signed, which orders the top half of the range backwards against the
     *  Rust port and gives the two a different tree shape for the same seed. */
    private static boolean outranks(long a, long b) {
        return Long.compareUnsigned(a, b) > 0;
    }

    private long nextPriority() {
        rngState = rngState * 6364136223846793005L + 1442695040888963407L;
        // SplitMix64 finalizer - decorrelate the priority from the key so
        // the tree keeps its expected O(log n) height. A bare LCG priority
        // stays correlated with any sibling LCG-derived key stream and
        // sorts the treap into a spine. Mirrors the Rust next_priority.
        long z = rngState;
        z = (z ^ (z >>> 30)) * 0xbf58476d1ce4e5b9L;
        z = (z ^ (z >>> 27)) * 0x94d049bb133111ebL;
        return z ^ (z >>> 31);
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
            if (outranks(node.left.priority, node.priority)) node = rotateRight(node);
            return new InsResult<>(node, r.replaced);
        } else {
            InsResult<K, V> r = ins(node.right, key, value, pri);
            node.right = r.root;
            if (outranks(node.right.priority, node.priority)) node = rotateLeft(node);
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
        if (outranks(left.priority, right.priority)) {
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
