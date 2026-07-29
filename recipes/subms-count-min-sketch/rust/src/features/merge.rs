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
#[path = "merge_tests.rs"]
mod tests;
