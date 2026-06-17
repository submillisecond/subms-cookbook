//! Element-wise merge of two sketches of identical shape.
//!
//! The base CMS uses conservative update: summing two sketches' cells
//! pointwise would over-count for keys that appeared in both inputs
//! (each sketch already absorbed the over-estimate damping). The safe
//! combiner is element-wise MAX, which preserves the invariant that
//! every cell is >= the true count for every key in the union.
//!
//! `merge_into(dst, src)` mutates `dst` in place. The reverse direction
//! is symmetric. Both sketches must have matching (depth, width); shape
//! mismatch is an error rather than a silent reshape.

use crate::CountMinSketch;

#[derive(Debug, PartialEq, Eq)]
pub enum MergeError {
    DepthMismatch { dst: usize, src: usize },
    WidthMismatch { dst: usize, src: usize },
}

impl core::fmt::Display for MergeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            MergeError::DepthMismatch { dst, src } => {
                write!(f, "depth mismatch: dst={dst}, src={src}")
            }
            MergeError::WidthMismatch { dst, src } => {
                write!(f, "width mismatch: dst={dst}, src={src}")
            }
        }
    }
}

impl std::error::Error for MergeError {}

/// Element-wise max-merge of `src` into `dst`. Both sketches must have
/// the same depth and width.
pub fn merge_into(dst: &mut CountMinSketch, src: &CountMinSketch) -> Result<(), MergeError> {
    if dst.depth() != src.depth() {
        return Err(MergeError::DepthMismatch {
            dst: dst.depth(),
            src: src.depth(),
        });
    }
    if dst.width() != src.width() {
        return Err(MergeError::WidthMismatch {
            dst: dst.width(),
            src: src.width(),
        });
    }
    dst.apply_paired_max(src);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_disjoint_keys_preserves_both_counts() {
        let mut a = CountMinSketch::new(5, 4096);
        let mut b = CountMinSketch::new(5, 4096);
        for _ in 0..200 {
            a.add("alpha");
        }
        for _ in 0..150 {
            b.add("beta");
        }
        merge_into(&mut a, &b).unwrap();
        assert!(a.estimate("alpha") >= 200);
        assert!(a.estimate("beta") >= 150);
    }

    #[test]
    fn merge_shared_key_takes_max_not_sum() {
        // a saw the key 100 times. b saw it 300 times. Pointwise max
        // gives ~300, pointwise sum would give ~400 - we want the former
        // because both sketches independently absorbed over-estimation.
        let mut a = CountMinSketch::new(5, 16384);
        let mut b = CountMinSketch::new(5, 16384);
        for _ in 0..100 {
            a.add("shared");
        }
        for _ in 0..300 {
            b.add("shared");
        }
        merge_into(&mut a, &b).unwrap();
        let est = a.estimate("shared");
        // Max under the conservative-update bound stays near 300, not 400.
        assert!(est >= 300, "expected >= 300, got {est}");
        assert!(est < 350, "expected close to 300, got {est}");
    }

    #[test]
    fn merge_with_empty_is_noop() {
        let mut a = CountMinSketch::new(5, 4096);
        let empty = CountMinSketch::new(5, 4096);
        for _ in 0..50 {
            a.add("x");
        }
        let before = a.estimate("x");
        merge_into(&mut a, &empty).unwrap();
        assert_eq!(a.estimate("x"), before);
    }

    #[test]
    fn depth_mismatch_errors() {
        let mut a = CountMinSketch::new(5, 4096);
        let b = CountMinSketch::new(7, 4096);
        let err = merge_into(&mut a, &b).unwrap_err();
        match err {
            MergeError::DepthMismatch { dst, src } => {
                assert_eq!(dst, 5);
                assert_eq!(src, 7);
            }
            other => panic!("expected depth mismatch, got {other:?}"),
        }
    }

    #[test]
    fn width_mismatch_errors() {
        let mut a = CountMinSketch::new(5, 4096);
        let b = CountMinSketch::new(5, 8192);
        let err = merge_into(&mut a, &b).unwrap_err();
        match err {
            MergeError::WidthMismatch { dst, src } => {
                assert_eq!(dst, 4096);
                assert_eq!(src, 8192);
            }
            other => panic!("expected width mismatch, got {other:?}"),
        }
    }

    #[test]
    fn merge_is_idempotent_when_src_already_dominated() {
        let mut a = CountMinSketch::new(5, 4096);
        let b = CountMinSketch::new(5, 4096);
        for _ in 0..50 {
            a.add("k");
        }
        // b is empty - all-zero cells - so max(a, b) == a.
        merge_into(&mut a, &b).unwrap();
        let once = a.estimate("k");
        merge_into(&mut a, &b).unwrap();
        let twice = a.estimate("k");
        assert_eq!(once, twice);
    }

    #[test]
    fn merged_keys_only_in_src_become_visible() {
        let mut a = CountMinSketch::new(5, 4096);
        let mut b = CountMinSketch::new(5, 4096);
        for _ in 0..75 {
            b.add("only-in-b");
        }
        assert_eq!(a.estimate("only-in-b"), 0);
        merge_into(&mut a, &b).unwrap();
        assert!(a.estimate("only-in-b") >= 75);
    }
}
