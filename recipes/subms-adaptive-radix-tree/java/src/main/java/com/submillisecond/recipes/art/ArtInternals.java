package com.submillisecond.recipes.art;

import java.io.ByteArrayOutputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Cross-package internal accessors used by the {@code features.*} subpackage.
 * Not part of the public API; subject to change.
 *
 * <p>The internal {@code Node} type isn't exposed; everything is surfaced as
 * collections of (key, value) or aggregate counters so the features package
 * never reaches into the node graph directly.
 */
public final class ArtInternals {

    private ArtInternals() {}

    /**
     * Walk the tree in byte-lex key order. Output buffers receive the same N
     * pairs, parallel-indexed.
     */
    public static <V> void collect(Art<V> tree, List<byte[]> keys, List<V> values) {
        collectRec(tree.root, new ByteArrayOutputStream(), keys, values);
    }

    private static <V> void collectRec(
            Art.Node<V> node, ByteArrayOutputStream prefix, List<byte[]> keys, List<V> values) {
        int before = prefix.size();
        prefix.write(node.prefix, 0, node.prefix.length); // path compression
        if (node.value != null) {
            keys.add(prefix.toByteArray());
            values.add(node.value);
        }
        for (Map.Entry<Byte, Art.Node<V>> e : sortedPairs(node)) {
            int beforeChild = prefix.size();
            prefix.write(e.getKey() & 0xff);
            collectRec(e.getValue(), prefix, keys, values);
            truncate(prefix, beforeChild);
        }
        truncate(prefix, before);
    }

    /** Bridge for the compaction feature's delete. */
    public static <V> V delete(Art<V> tree, byte[] key) {
        return tree.deleteValue(key);
    }

    /**
     * Counters over the node-shape distribution, indexed
     * {@code [Node4, Node16, Node48, Node256]}.
     */
    public static <V> int[] nodeTypeCounts(Art<V> tree) {
        int[] acc = new int[4];
        countWalk(tree.root, acc);
        return acc;
    }

    private static <V> void countWalk(Art.Node<V> node, int[] acc) {
        switch (node.children.kind()) {
            case NODE4 -> acc[0]++;
            case NODE16 -> acc[1]++;
            case NODE48 -> acc[2]++;
            case NODE256 -> acc[3]++;
        }
        for (Map.Entry<Byte, Art.Node<V>> e : sortedPairs(node)) {
            countWalk(e.getValue(), acc);
        }
    }

    /**
     * Compaction pass entry point. Returns the count of structural changes
     * (pruned empty subtrees + single-child merges + shape demotions).
     */
    public static <V> int compact(Art<V> tree) {
        int[] changes = new int[1];
        compactRec(tree.root, changes);
        return changes[0];
    }

    private static <V> void compactRec(Art.Node<V> node, int[] changes) {
        // Depth-first: settle descendants before this node's shape.
        for (Map.Entry<Byte, Art.Node<V>> e : sortedPairs(node)) {
            compactRec(e.getValue(), changes);
        }

        // Prune children whose subtree carries no remaining values.
        List<Byte> toDrop = new ArrayList<>();
        for (Map.Entry<Byte, Art.Node<V>> e : sortedPairs(node)) {
            Art.Node<V> child = e.getValue();
            if (child.value == null && child.children.size() == 0) {
                toDrop.add(e.getKey());
            }
        }
        for (Byte b : toDrop) {
            node.children.remove(b);
            changes[0]++;
        }

        // Collapse a value-less single-child node into its child, re-extending the
        // compressed prefix - the delete-side counterpart to insert's split.
        if (node.value == null && node.children.size() == 1) {
            mergeSingleChild(node);
            changes[0]++;
        }

        // Demote to the smallest layout the remaining occupancy fits.
        if (maybeShrink(node)) {
            changes[0]++;
        }
    }

    private static <V> void mergeSingleChild(Art.Node<V> node) {
        List<Map.Entry<Byte, Art.Node<V>>> pairs = Art.takeAllSorted(node);
        Map.Entry<Byte, Art.Node<V>> e = pairs.get(0);
        byte edge = e.getKey();
        Art.Node<V> child = e.getValue();
        byte[] np = new byte[node.prefix.length + 1 + child.prefix.length];
        System.arraycopy(node.prefix, 0, np, 0, node.prefix.length);
        np[node.prefix.length] = edge;
        System.arraycopy(child.prefix, 0, np, node.prefix.length + 1, child.prefix.length);
        // Node absorbs the child (it is referenced by its parent, so we cannot
        // swap the object - we copy the child's fields in).
        node.prefix = np;
        node.value = child.value;
        node.children = child.children;
    }

    private static <V> boolean maybeShrink(Art.Node<V> node) {
        if (rank(node.children.kind()) <= neededRank(node.children.size())) {
            return false;
        }
        // Re-inserting into a fresh Node4 auto-grows to the minimal layout.
        for (Map.Entry<Byte, Art.Node<V>> e : Art.takeAllSorted(node)) {
            node.addChild(e.getKey(), e.getValue());
        }
        return true;
    }

    private static int rank(Art.NodeKind kind) {
        return switch (kind) {
            case NODE4 -> 0;
            case NODE16 -> 1;
            case NODE48 -> 2;
            case NODE256 -> 3;
        };
    }

    private static int neededRank(int occupancy) {
        if (occupancy <= 4) {
            return 0;
        }
        if (occupancy <= 16) {
            return 1;
        }
        if (occupancy <= 48) {
            return 2;
        }
        return 3;
    }

    /** Byte-sorted {@code (byte, child)} pairs of a node. */

    // ---- node-level bridge, for the v2 serializer -------------------------
    //
    // The flat (key, value) `collect` above cannot express the on-disk format:
    // v2 streams one record per NODE carrying its path-compressed prefix, and
    // interleaves each child's edge byte with that child's whole record. A
    // writer therefore has to drive the recursion itself.
    //
    // It still must not see `Node` - that is what keeps the features package off
    // the internals - so the graph is handed out as a navigable REFERENCE with
    // exactly the four questions a serializer asks, and nothing else.
    //
    // This exists because the Java port shipped v1 (one record per key byte)
    // while Rust shipped v2, and this file's javadoc claimed the two were
    // byte-equivalent on disk. They were mutually unreadable.

    /** Read-only view of one node, for writing a v2 stream. */
    public static final class NodeRef<V> {
        private final Art.Node<V> node;

        private NodeRef(Art.Node<V> node) {
            this.node = node;
        }

        public byte[] prefix() {
            return node.prefix;
        }

        public V value() {
            return node.value;
        }

        /** Child edges in ascending unsigned order. */
        public byte[] childBytes() {
            List<Map.Entry<Byte, Art.Node<V>>> pairs = sortedPairs(node);
            byte[] bytes = new byte[pairs.size()];
            for (int i = 0; i < pairs.size(); i++) {
                bytes[i] = pairs.get(i).getKey();
            }
            return bytes;
        }

        public NodeRef<V> child(byte b) {
            Art.Node<V> c = node.children.get(b);
            return c == null ? null : new NodeRef<>(c);
        }
    }

    /** Root of the node graph, read-only. */
    public static <V> NodeRef<V> rootRef(Art<V> tree) {
        return new NodeRef<>(tree.root);
    }

    /**
     * Writable handle to one node, for rebuilding a tree from a v2 stream.
     * A reader can set this node's contents and attach a child, nothing else.
     */
    public static final class NodeHandle<V> {
        private final Art.Node<V> node;

        private NodeHandle(Art.Node<V> node) {
            this.node = node;
        }

        public void set(byte[] prefix, V value) {
            node.prefix = prefix == null ? Art.EMPTY : prefix;
            node.value = value;
        }

        /** Attach (or fetch) the child on edge {@code b}. */
        public NodeHandle<V> child(byte b) {
            Art.Node<V> existing = node.children.get(b);
            if (existing == null) {
                existing = new Art.Node<>();
                node.addChild(b, existing);
            }
            return new NodeHandle<>(existing);
        }
    }

    /** Root handle of an empty tree, for a reader to populate. */
    public static <V> NodeHandle<V> rootHandle(Art<V> tree) {
        return new NodeHandle<>(tree.root);
    }

    /**
     * Set the entry count after a rebuild. The stream carries it in the header
     * rather than the reader counting values, so a truncated stream is a read
     * error and not a silently short tree.
     */
    public static <V> void setSize(Art<V> tree, int size) {
        tree.size = size;
    }

    static <V> List<Map.Entry<Byte, Art.Node<V>>> sortedPairs(Art.Node<V> node) {
        List<Map.Entry<Byte, Art.Node<V>>> pairs = new ArrayList<>();
        node.children.appendPairs(pairs);
        pairs.sort(Map.Entry.comparingByKey());
        return pairs;
    }

    private static void truncate(ByteArrayOutputStream b, int len) {
        byte[] saved = b.toByteArray();
        b.reset();
        b.write(saved, 0, len);
    }
}
