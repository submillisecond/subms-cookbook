package com.submillisecond.recipes.cms.features;

import com.submillisecond.recipes.cms.CountMinSketch;

/**
 * Element-wise merge of two sketches of identical shape.
 *
 * <p>The base CMS uses conservative update: summing two sketches' cells
 * pointwise would over-count for keys that appeared in both inputs (each
 * sketch already absorbed the over-estimate damping). The safe combiner
 * is element-wise MAX, which preserves the invariant that every cell is
 * >= the true count for every key in the union.
 *
 * <p>{@link #mergeInto(CountMinSketch, CountMinSketch)} mutates the
 * destination in place. Both sketches must have matching (depth, width);
 * shape mismatch throws {@link MergeException} rather than silently
 * reshaping.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_count_min_sketch::merge_into}.
 */
public final class Merge {

    private Merge() {}

    public static final class MergeException extends RuntimeException {
        private static final long serialVersionUID = 1L;
        public MergeException(String msg) { super(msg); }
    }

    /**
     * Element-wise max-merge of {@code src} into {@code dst}.
     *
     * @throws MergeException when depths or widths differ.
     */
    public static void mergeInto(CountMinSketch dst, CountMinSketch src) {
        if (dst.depth() != src.depth()) {
            throw new MergeException(
                "depth mismatch: dst=" + dst.depth() + ", src=" + src.depth()
            );
        }
        if (dst.width() != src.width()) {
            throw new MergeException(
                "width mismatch: dst=" + dst.width() + ", src=" + src.width()
            );
        }
        dst.applyPairedMax(src);
    }
}
