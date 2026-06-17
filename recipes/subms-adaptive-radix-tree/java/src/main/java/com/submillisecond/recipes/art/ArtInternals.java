package com.submillisecond.recipes.art;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;

/**
 * Cross-package internal accessors used by the {@code features.*}
 * subpackage. Not part of the public API; subject to change.
 *
 * <p>The internal {@code Node} type isn't exposed; everything is
 * surfaced as collections of (key, value) or aggregate counters so the
 * features package never reaches into the node graph directly.
 */
public final class ArtInternals {

    private ArtInternals() {}

    /** Walk the tree in byte-lex key order. Output buffers receive the
     *  same N pairs, parallel-indexed. */
    public static <V> void collect(Art<V> tree, List<byte[]> keys, List<V> values) {
        collectRec(tree.root, new ByteArrayOutputStream(), keys, values);
    }

    private static <V> void collectRec(
            Art.Node<V> node,
            ByteArrayOutputStream prefix,
            List<byte[]> keys,
            List<V> values) {
        if (node.value != null) {
            keys.add(prefix.toByteArray());
            values.add(node.value);
        }
        TreeMap<Byte, Art.Node<V>> sorted = sortedChildren(node);
        for (Map.Entry<Byte, Art.Node<V>> e : sorted.entrySet()) {
            int beforeLen = prefix.size();
            prefix.write(e.getKey() & 0xff);
            collectRec(e.getValue(), prefix, keys, values);
            byte[] saved = prefix.toByteArray();
            prefix.reset();
            prefix.write(saved, 0, beforeLen);
        }
    }

    /** Bridge for the compaction feature's delete. */
    public static <V> V delete(Art<V> tree, byte[] key) {
        return tree.deleteValue(key);
    }

    /** Counter pair (Small / Full) over the tree's node-shape distribution. */
    public static <V> int[] nodeTypeCounts(Art<V> tree) {
        int[] acc = new int[2];
        countWalk(tree.root, acc);
        return acc;
    }

    private static <V> void countWalk(Art.Node<V> node, int[] acc) {
        if (node.children instanceof Art.SmallChildren) {
            acc[0]++;
        } else {
            acc[1]++;
        }
        for (Art.Node<V> c : sortedChildren(node).values()) {
            countWalk(c, acc);
        }
    }

    @SuppressWarnings("unchecked")
    private static <V> TreeMap<Byte, Art.Node<V>> sortedChildren(Art.Node<V> node) {
        TreeMap<Byte, Art.Node<V>> sorted = new TreeMap<>();
        Object kids = node.children;
        if (kids instanceof Art.SmallChildren) {
            Art.SmallChildren<V> s = (Art.SmallChildren<V>) kids;
            for (int i = 0; i < s.count; i++) {
                sorted.put(s.keys[i], s.children[i]);
            }
        } else {
            for (Map.Entry<Byte, Art.Node<V>> e : ((Map<Byte, Art.Node<V>>) kids).entrySet()) {
                sorted.put(e.getKey(), e.getValue());
            }
        }
        return sorted;
    }

    /** Compaction pass entry point. Returns the count of structural
     *  changes (shape-shrink + pruned empty subtree) for observability. */
    public static <V> int compact(Art<V> tree) {
        int[] changes = new int[1];
        compactRec(tree.root, changes);
        return changes[0];
    }

    @SuppressWarnings("unchecked")
    private static <V> void compactRec(Art.Node<V> node, int[] changes) {
        // Depth-first: compact descendants before this node's shape.
        for (Art.Node<V> c : new ArrayList<>(sortedChildren(node).values())) {
            compactRec(c, changes);
        }

        // Prune children whose subtree carries no remaining values.
        List<Byte> toDrop = new ArrayList<>();
        for (Map.Entry<Byte, Art.Node<V>> e : sortedChildren(node).entrySet()) {
            Art.Node<V> child = e.getValue();
            if (child.value == null && isLeafShape(child)) {
                toDrop.add(e.getKey());
            }
        }
        for (Byte b : toDrop) {
            removeChild(node, b);
            changes[0]++;
        }

        // Shrink shape: Full -> Small when occupancy <= 4.
        if (!(node.children instanceof Art.SmallChildren)) {
            Map<Byte, Art.Node<V>> map = (Map<Byte, Art.Node<V>>) node.children;
            if (map.size() <= 4) {
                shrinkToSmall(node);
                changes[0]++;
            }
        }
    }

    @SuppressWarnings("unchecked")
    private static <V> boolean isLeafShape(Art.Node<V> node) {
        Object kids = node.children;
        if (kids instanceof Art.SmallChildren) {
            return ((Art.SmallChildren<V>) kids).count == 0;
        }
        return ((Map<Byte, Art.Node<V>>) kids).isEmpty();
    }

    @SuppressWarnings("unchecked")
    private static <V> void removeChild(Art.Node<V> node, byte b) {
        Object kids = node.children;
        if (kids instanceof Art.SmallChildren) {
            Art.SmallChildren<V> s = (Art.SmallChildren<V>) kids;
            for (int i = 0; i < s.count; i++) {
                if (s.keys[i] == b) {
                    int last = s.count - 1;
                    s.keys[i] = s.keys[last];
                    s.children[i] = s.children[last];
                    s.children[last] = null;
                    s.keys[last] = 0;
                    s.count--;
                    return;
                }
            }
        } else {
            ((Map<Byte, Art.Node<V>>) kids).remove(b);
        }
    }

    @SuppressWarnings("unchecked")
    private static <V> void shrinkToSmall(Art.Node<V> node) {
        TreeMap<Byte, Art.Node<V>> sorted = sortedChildren(node);
        Art.SmallChildren<V> s = new Art.SmallChildren<>();
        int i = 0;
        for (Map.Entry<Byte, Art.Node<V>> e : sorted.entrySet()) {
            s.keys[i] = e.getKey();
            s.children[i] = e.getValue();
            i++;
        }
        s.count = sorted.size();
        node.children = s;
    }
}
