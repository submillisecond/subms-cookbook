//! Fold sketches built in parallel back into one.
//!
//! `merge_into(dst, src)` adds `src` into `dst` cell by cell. Addition is the
//! combiner that keeps the Count-Min guarantee across the union: every cell of
//! each input is already `>=` that input's true count for any key landing
//! there, so the sum is `>=` the sum of the true counts.
//!
//! `merge_disjoint_into` takes the element-wise maximum instead. That is only
//! sound when the inputs partition the KEY space - one shard per symbol range,
//! say - because max of two per-shard counts is an under-count the moment a key
//! appears on both sides. It is tighter than addition when the precondition
//! holds, and silently wrong when it does not.
//!
//! Both mutate `dst` in place and require matching shape and seed; a mismatch
//! is an error rather than a silent reshape.

use crate::CountMinSketch;

#[derive(Debug, PartialEq, Eq)]
pub enum MergeError {
    DepthMismatch { dst: usize, src: usize },
    WidthMismatch { dst: usize, src: usize },
    SeedMismatch { dst: u64, src: u64 },
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
            MergeError::SeedMismatch { dst, src } => {
                write!(f, "seed mismatch: dst={dst}, src={src}")
            }
        }
    }
}

impl std::error::Error for MergeError {}

/// Element-wise saturating sum of `src` into `dst`. Preserves
/// `estimate >= true count` over the union of the two streams.
pub fn merge_into(dst: &mut CountMinSketch, src: &CountMinSketch) -> Result<(), MergeError> {
    check(dst, src)?;
    dst.apply_paired(src, true);
    Ok(())
}

/// Element-wise maximum of `src` into `dst`. Sound only when the two sketches
/// saw disjoint key sets; on overlapping keys it under-counts.
pub fn merge_disjoint_into(
    dst: &mut CountMinSketch,
    src: &CountMinSketch,
) -> Result<(), MergeError> {
    check(dst, src)?;
    dst.apply_paired(src, false);
    Ok(())
}

fn check(dst: &CountMinSketch, src: &CountMinSketch) -> Result<(), MergeError> {
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
    if dst.seed() != src.seed() {
        return Err(MergeError::SeedMismatch {
            dst: dst.seed(),
            src: src.seed(),
        });
    }
    Ok(())
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
