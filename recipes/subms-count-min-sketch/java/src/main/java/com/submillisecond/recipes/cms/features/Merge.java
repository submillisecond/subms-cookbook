package com.submillisecond.recipes.cms.features;

import com.submillisecond.recipes.cms.CountMinSketch;

/**
 * Fold sketches built in parallel back into one.
 *
 * <p>{@link #mergeInto(CountMinSketch, CountMinSketch)} adds {@code src} into
 * {@code dst} cell by cell. Addition is the combiner that keeps the Count-Min
 * guarantee across the union: every cell of each input is already {@code >=}
 * that input's true count for any key landing there, so the sum is {@code >=}
 * the sum of the true counts.
 *
 * <p>{@link #mergeDisjointInto(CountMinSketch, CountMinSketch)} takes the
 * element-wise maximum instead. That is only sound when the inputs partition
 * the KEY space - one shard per symbol range, say - because max of two
 * per-shard counts is an under-count the moment a key appears on both sides.
 * It is tighter than addition when the precondition holds, and silently wrong
 * when it does not.
 *
 * <p>Both mutate the destination in place and require matching shape and seed;
 * a mismatch throws {@link MergeException} rather than silently reshaping.
 *
 * <p>Behaviour-equivalent to the Rust siblings
 * {@code subms_count_min_sketch::merge_into} and {@code merge_disjoint_into}.
 */
public final class Merge {

    private Merge() {}

    public static final class MergeException extends RuntimeException {
        private static final long serialVersionUID = 1L;
        public MergeException(String msg) { super(msg); }
    }

    /**
     * Element-wise saturating sum of {@code src} into {@code dst}. Preserves
     * {@code estimate >= true count} over the union of the two streams.
     *
     * @throws MergeException when depth, width or seed differ.
     */
    public static void mergeInto(CountMinSketch dst, CountMinSketch src) {
        check(dst, src);
        dst.applyPaired(src, true);
    }

    /**
     * Element-wise maximum of {@code src} into {@code dst}. Sound only when
     * the two sketches saw disjoint key sets; on overlapping keys it
     * under-counts.
     *
     * @throws MergeException when depth, width or seed differ.
     */
    public static void mergeDisjointInto(CountMinSketch dst, CountMinSketch src) {
        check(dst, src);
        dst.applyPaired(src, false);
    }

    private static void check(CountMinSketch dst, CountMinSketch src) {
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
        if (dst.seed() != src.seed()) {
            throw new MergeException(
                "seed mismatch: dst=" + dst.seed() + ", src=" + src.seed()
            );
        }
    }
}
