package com.submillisecond.recipes.art;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;

/**
 * Adaptive Radix Tree over byte-string keys - Leis et al., 2013.
 *
 * <p>Child storage adapts to fan-out: {@code Node4} (up to 4 children, linear
 * scan), {@code Node16} (up to 16), {@code Node48} (a 256-entry byte index into
 * 48 slots), {@code Node256} (direct 256-way). A node promotes to the next size
 * when it fills and demotes when {@code Compaction} shrinks it. Path compression
 * collapses a run of single-child bytes into one node's {@code prefix}; a
 * diverging insert splits the node at the first mismatch.
 */
public final class Art<V> {

    static final byte[] EMPTY = new byte[0];

    enum NodeKind {
        NODE4,
        NODE16,
        NODE48,
        NODE256
    }

    static final class Node<V> {
        /** Path-compressed bytes shared by every key under this node. */
        byte[] prefix = EMPTY;
        V value;
        Children<V> children = new Node4<>();

        /** Grow-and-insert: promote to the next node size first if full. */
        void addChild(byte b, Node<V> child) {
            if (children.isFull()) {
                children = children.grow();
            }
            children.put(b, child);
        }
    }

    final Node<V> root = new Node<>();
    int size;

    public int size() {
        return size;
    }

    public boolean isEmpty() {
        return size == 0;
    }

    public V insert(byte[] key, V value) {
        boolean[] added = new boolean[1];
        V prior = insertRec(root, key, 0, value, added);
        if (added[0]) {
            size++;
        }
        return prior;
    }

    private V insertRec(Node<V> node, byte[] key, int depth, V value, boolean[] added) {
        int common = commonPrefixLen(node.prefix, key, depth);
        if (common < node.prefix.length) {
            splitNode(node, key, depth, value, common);
            added[0] = true;
            return null;
        }
        depth += node.prefix.length;
        if (depth == key.length) {
            V prior = node.value;
            node.value = value;
            added[0] = prior == null;
            return prior;
        }
        byte b = key[depth];
        Node<V> child = node.children.get(b);
        if (child != null) {
            return insertRec(child, key, depth + 1, value, added);
        }
        Node<V> leaf = new Node<>();
        leaf.prefix = Arrays.copyOfRange(key, depth + 1, key.length);
        leaf.value = value;
        node.addChild(b, leaf);
        added[0] = true;
        return null;
    }

    /**
     * Split {@code node} at prefix position {@code common}: a fresh parent takes
     * {@code prefix[..common]}, the old node drops to a child under byte
     * {@code prefix[common]} with {@code prefix[common+1..]}, and the new key
     * branches beside it (or terminates in the parent).
     */
    private void splitNode(Node<V> node, byte[] key, int depth, V value, int common) {
        Node<V> child = new Node<>();
        child.prefix = Arrays.copyOfRange(node.prefix, common + 1, node.prefix.length);
        child.value = node.value;
        child.children = node.children;
        byte childEdge = node.prefix[common];

        node.prefix = Arrays.copyOfRange(node.prefix, 0, common);
        node.value = null;
        node.children = new Node4<>();
        node.children.put(childEdge, child);

        int keyPos = depth + common;
        if (keyPos == key.length) {
            node.value = value;
        } else {
            Node<V> leaf = new Node<>();
            leaf.prefix = Arrays.copyOfRange(key, keyPos + 1, key.length);
            leaf.value = value;
            node.addChild(key[keyPos], leaf);
        }
    }

    public V get(byte[] key) {
        Node<V> node = root;
        int depth = 0;
        while (true) {
            int p = node.prefix.length;
            if (key.length < depth + p || !regionEquals(key, depth, node.prefix)) {
                return null;
            }
            depth += p;
            if (depth == key.length) {
                return node.value;
            }
            node = node.children.get(key[depth]);
            if (node == null) {
                return null;
            }
            depth += 1;
        }
    }

    /**
     * Package-private: features call this to remove a value. The path is left in
     * place; {@code Compaction} reclaims it.
     */
    V deleteValue(byte[] key) {
        Node<V> node = root;
        int depth = 0;
        while (true) {
            int p = node.prefix.length;
            if (key.length < depth + p || !regionEquals(key, depth, node.prefix)) {
                return null;
            }
            depth += p;
            if (depth == key.length) {
                V prior = node.value;
                if (prior != null) {
                    node.value = null;
                    size--;
                }
                return prior;
            }
            node = node.children.get(key[depth]);
            if (node == null) {
                return null;
            }
            depth += 1;
        }
    }

    static int commonPrefixLen(byte[] prefix, byte[] key, int depth) {
        int n = Math.min(prefix.length, key.length - depth);
        int i = 0;
        while (i < n && prefix[i] == key[depth + i]) {
            i++;
        }
        return i;
    }

    static boolean regionEquals(byte[] key, int depth, byte[] prefix) {
        for (int i = 0; i < prefix.length; i++) {
            if (key[depth + i] != prefix[i]) {
                return false;
            }
        }
        return true;
    }

    // ----- Adaptive child layouts -----

    abstract static class Children<V> {
        abstract Node<V> get(byte b);

        abstract boolean isFull();

        abstract Children<V> grow();

        /** Insert a child under {@code b} (not already present); room assumed. */
        abstract void put(byte b, Node<V> child);

        abstract Node<V> remove(byte b);

        abstract int size();

        abstract NodeKind kind();

        /** Append every {@code (byte, child)} pair, unsorted, to {@code out}. */
        abstract void appendPairs(List<Map.Entry<Byte, Node<V>>> out);
    }

    static final class Node4<V> extends Children<V> {
        final byte[] keys = new byte[4];
        @SuppressWarnings("unchecked")
        final Node<V>[] child = (Node<V>[]) new Node[4];
        int count;

        Node<V> get(byte b) {
            for (int i = 0; i < count; i++) {
                if (keys[i] == b) {
                    return child[i];
                }
            }
            return null;
        }

        boolean isFull() {
            return count == 4;
        }

        Children<V> grow() {
            Node16<V> n = new Node16<>();
            System.arraycopy(keys, 0, n.keys, 0, count);
            System.arraycopy(child, 0, n.child, 0, count);
            n.count = count;
            return n;
        }

        void put(byte b, Node<V> c) {
            keys[count] = b;
            child[count] = c;
            count++;
        }

        Node<V> remove(byte b) {
            for (int i = 0; i < count; i++) {
                if (keys[i] == b) {
                    Node<V> removed = child[i];
                    keys[i] = keys[count - 1];
                    child[i] = child[count - 1];
                    child[count - 1] = null;
                    keys[count - 1] = 0;
                    count--;
                    return removed;
                }
            }
            return null;
        }

        int size() {
            return count;
        }

        NodeKind kind() {
            return NodeKind.NODE4;
        }

        void appendPairs(List<Map.Entry<Byte, Node<V>>> out) {
            for (int i = 0; i < count; i++) {
                out.add(Map.entry(keys[i], child[i]));
            }
        }
    }

    static final class Node16<V> extends Children<V> {
        final byte[] keys = new byte[16];
        @SuppressWarnings("unchecked")
        final Node<V>[] child = (Node<V>[]) new Node[16];
        int count;

        Node<V> get(byte b) {
            for (int i = 0; i < count; i++) {
                if (keys[i] == b) {
                    return child[i];
                }
            }
            return null;
        }

        boolean isFull() {
            return count == 16;
        }

        Children<V> grow() {
            Node48<V> n = new Node48<>();
            for (int i = 0; i < count; i++) {
                n.child[i] = child[i];
                n.index[keys[i] & 0xff] = (byte) (i + 1);
            }
            n.count = count;
            return n;
        }

        void put(byte b, Node<V> c) {
            keys[count] = b;
            child[count] = c;
            count++;
        }

        Node<V> remove(byte b) {
            for (int i = 0; i < count; i++) {
                if (keys[i] == b) {
                    Node<V> removed = child[i];
                    keys[i] = keys[count - 1];
                    child[i] = child[count - 1];
                    child[count - 1] = null;
                    keys[count - 1] = 0;
                    count--;
                    return removed;
                }
            }
            return null;
        }

        int size() {
            return count;
        }

        NodeKind kind() {
            return NodeKind.NODE16;
        }

        void appendPairs(List<Map.Entry<Byte, Node<V>>> out) {
            for (int i = 0; i < count; i++) {
                out.add(Map.entry(keys[i], child[i]));
            }
        }
    }

    static final class Node48<V> extends Children<V> {
        /** {@code index[b] == 0} means absent; else child is {@code child[index[b]-1]}. */
        final byte[] index = new byte[256];
        @SuppressWarnings("unchecked")
        final Node<V>[] child = (Node<V>[]) new Node[48];
        int count;

        Node<V> get(byte b) {
            int slot = index[b & 0xff] & 0xff;
            return slot == 0 ? null : child[slot - 1];
        }

        boolean isFull() {
            return count == 48;
        }

        Children<V> grow() {
            Node256<V> n = new Node256<>();
            for (int b = 0; b < 256; b++) {
                int slot = index[b] & 0xff;
                if (slot != 0) {
                    n.child[b] = child[slot - 1];
                }
            }
            n.count = count;
            return n;
        }

        void put(byte b, Node<V> c) {
            child[count] = c;
            index[b & 0xff] = (byte) (count + 1);
            count++;
        }

        Node<V> remove(byte b) {
            int slot = index[b & 0xff] & 0xff;
            if (slot == 0) {
                return null;
            }
            Node<V> removed = child[slot - 1];
            child[slot - 1] = null;
            index[b & 0xff] = 0;
            count--;
            return removed;
        }

        int size() {
            return count;
        }

        NodeKind kind() {
            return NodeKind.NODE48;
        }

        void appendPairs(List<Map.Entry<Byte, Node<V>>> out) {
            for (int b = 0; b < 256; b++) {
                int slot = index[b] & 0xff;
                if (slot != 0) {
                    out.add(Map.entry((byte) b, child[slot - 1]));
                }
            }
        }
    }

    static final class Node256<V> extends Children<V> {
        @SuppressWarnings("unchecked")
        final Node<V>[] child = (Node<V>[]) new Node[256];
        int count;

        Node<V> get(byte b) {
            return child[b & 0xff];
        }

        boolean isFull() {
            return false;
        }

        Children<V> grow() {
            return this;
        }

        void put(byte b, Node<V> c) {
            child[b & 0xff] = c;
            count++;
        }

        Node<V> remove(byte b) {
            Node<V> removed = child[b & 0xff];
            if (removed != null) {
                child[b & 0xff] = null;
                count--;
            }
            return removed;
        }

        int size() {
            return count;
        }

        NodeKind kind() {
            return NodeKind.NODE256;
        }

        void appendPairs(List<Map.Entry<Byte, Node<V>>> out) {
            for (int b = 0; b < 256; b++) {
                if (child[b] != null) {
                    out.add(Map.entry((byte) b, child[b]));
                }
            }
        }
    }

    /** Drain a node's children into a byte-sorted list, leaving it empty. */
    static <V> List<Map.Entry<Byte, Node<V>>> takeAllSorted(Node<V> node) {
        List<Map.Entry<Byte, Node<V>>> pairs = new ArrayList<>();
        node.children.appendPairs(pairs);
        pairs.sort(Map.Entry.comparingByKey());
        node.children = new Node4<>();
        return pairs;
    }
}
