//! Sum two histograms with identical shape.
//!
//! Two histograms can be merged if they share `sub_count_bits` (same
//! significant-digit precision). The merged result is byte-equivalent
//! to recording every value into a single histogram, because each
//! value maps to the same bucket index regardless of which histogram
//! it landed in.
//!
//! Mismatched shapes return `Err` rather than producing a silent
//! mis-aligned merge. Re-record into a fresh histogram at the target
//! precision if you need to merge across precisions.

use crate::HdrHistogram;

/// Sum `src` into `dst`. Returns `Err` if the shapes don't match.
pub fn merge(dst: &mut HdrHistogram, src: &HdrHistogram) -> Result<(), &'static str> {
    dst.add_counts_from(src)
}

#[cfg(test)]
#[path = "merge_tests.rs"]
mod tests;
