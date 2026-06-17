package com.submillisecond.recipes.art.features;

import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;

import com.submillisecond.recipes.art.Art;
import com.submillisecond.recipes.art.ArtInternals;

/**
 * In-order scan between two byte-lex bounds.
 *
 * <p>Ordering is byte-lexicographic. Two consequences worth naming up
 * front:
 * <ul>
 *   <li>UTF-8 strings compare the same way as their UTF-8 byte
 *       sequences in this scheme. For ASCII-only keys the order matches
 *       what a reader would expect from a dictionary. Non-ASCII
 *       multi-byte sequences sort by their codepoint encoding, which is
 *       NOT a locale-aware collation.</li>
 *   <li>Empty key is the minimum element.</li>
 * </ul>
 *
 * <p>Bounds are independently inclusive / exclusive / unbounded.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_adaptive_radix_tree::features::range_scan}.
 */
public final class RangeScan {

    private RangeScan() {}

    public enum BoundKind { UNBOUNDED, INCLUDED, EXCLUDED }

    public static final class Bound {
        public final BoundKind kind;
        public final byte[] value;
        private Bound(BoundKind kind, byte[] value) {
            this.kind = kind;
            this.value = value;
        }
        public static Bound unbounded() { return new Bound(BoundKind.UNBOUNDED, null); }
        public static Bound included(byte[] value) { return new Bound(BoundKind.INCLUDED, value); }
        public static Bound excluded(byte[] value) { return new Bound(BoundKind.EXCLUDED, value); }
    }

    public static final class Entry<V> {
        public final byte[] key;
        public final V value;
        public Entry(byte[] key, V value) {
            this.key = key;
            this.value = value;
        }
    }

    public static <V> List<Entry<V>> range(Art<V> tree, Bound from, Bound to) {
        List<byte[]> keys = new ArrayList<>();
        List<V> values = new ArrayList<>();
        ArtInternals.collect(tree, keys, values);
        // collect() already produces byte-lex order; filter to bounds.
        List<Entry<V>> out = new ArrayList<>();
        Comparator<byte[]> cmp = RangeScan::lex;
        for (int i = 0; i < keys.size(); i++) {
            byte[] k = keys.get(i);
            boolean fromOk = switch (from.kind) {
                case UNBOUNDED -> true;
                case INCLUDED -> cmp.compare(k, from.value) >= 0;
                case EXCLUDED -> cmp.compare(k, from.value) > 0;
            };
            if (!fromOk) continue;
            boolean toOk = switch (to.kind) {
                case UNBOUNDED -> true;
                case INCLUDED -> cmp.compare(k, to.value) <= 0;
                case EXCLUDED -> cmp.compare(k, to.value) < 0;
            };
            if (!toOk) continue;
            out.add(new Entry<>(k, values.get(i)));
        }
        return out;
    }

    private static int lex(byte[] a, byte[] b) {
        int len = Math.min(a.length, b.length);
        for (int i = 0; i < len; i++) {
            int ai = a[i] & 0xff;
            int bi = b[i] & 0xff;
            if (ai != bi) return Integer.compare(ai, bi);
        }
        return Integer.compare(a.length, b.length);
    }
}
