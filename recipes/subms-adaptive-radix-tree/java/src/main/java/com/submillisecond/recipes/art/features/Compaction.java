package com.submillisecond.recipes.art.features;

import com.submillisecond.recipes.art.Art;
import com.submillisecond.recipes.art.ArtInternals;

/**
 * Memory-recovery passes for an ART that has been through a bulk
 * delete. The base tree doesn't expose deletion in its public API, so
 * this module pairs both halves:
 *
 * <ul>
 *   <li>{@link #delete(Art, byte[])} clears the value at {@code key},
 *       leaving the path intact (the byte path may still be costly).</li>
 *   <li>{@link #compact(Art)} walks the tree post-delete, shrinks any
 *       Full node whose occupancy fits into a Small (&lt;= 4 children),
 *       and prunes subtrees that hold no remaining values.</li>
 * </ul>
 *
 * <p>Idempotent: a second {@code compact()} does nothing visible.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_adaptive_radix_tree::features::compaction}.
 */
public final class Compaction {

    private Compaction() {}

    public static <V> V delete(Art<V> tree, byte[] key) {
        return ArtInternals.delete(tree, key);
    }

    /** Returns the number of structural changes (shape-shrink +
     *  pruned empty subtree). 0 means the tree was already compact. */
    public static <V> int compact(Art<V> tree) {
        return ArtInternals.compact(tree);
    }
}
