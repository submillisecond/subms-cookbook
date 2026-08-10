//! Sliding-window CMS as a ring of N sub-sketches.
//!
//! Each sub-sketch covers one time slice (the caller defines the time
//! unit by when they call `tick()`). `add()` writes only to the
//! current slice. `estimate()` sums the per-slice estimates, which
//! upper-bounds the true count over the window. `tick()` advances the
//! ring, clearing the now-current slice.
//!
//! Notes:
//! - Estimates summed across slices preserve the "always >= true count"
//!   property but lose tightness vs a single CMS of the same total
//!   width (the windowed shape is not a CMS in the linear-algebra
//!   sense - conservative-update is non-additive). Treat the bound as
//!   advisory rather than tight.
//! - `slices` defaults to >= 2; one slice degenerates to the base CMS.

use crate::CountMinSketch;

pub struct WindowedCountMinSketch {
    sketches: Vec<CountMinSketch>,
    head: usize,
}

impl WindowedCountMinSketch {
    /// `slices` sub-sketches each of shape (`depth`, `width`).
    pub fn new(slices: usize, depth: usize, width: usize) -> Self {
        Self::with_seed(slices, depth, width, 0)
    }

    pub fn with_seed(slices: usize, depth: usize, width: usize, seed: u64) -> Self {
        let n = slices.max(2);
        let sketches = (0..n)
            .map(|_| CountMinSketch::with_seed(depth, width, seed))
            .collect();
        Self { sketches, head: 0 }
    }

    pub fn slices(&self) -> usize {
        self.sketches.len()
    }
    pub fn depth(&self) -> usize {
        self.sketches[0].depth()
    }
    pub fn width(&self) -> usize {
        self.sketches[0].width()
    }

    /// Weight held across the whole window, exactly.
    pub fn total(&self) -> u64 {
        self.sketches
            .iter()
            .fold(0u64, |acc, s| acc.saturating_add(s.total()))
    }

    /// Footprint across every slice. A window costs `slices` times a base
    /// sketch, which is the price of aging counts out without decay maths.
    pub fn heap_bytes(&self) -> usize {
        self.sketches.iter().map(|s| s.heap_bytes()).sum()
    }

    pub fn add(&mut self, key: &str) {
        self.sketches[self.head].add(key);
    }

    /// Weighted add into the current slice.
    pub fn add_n(&mut self, key: &str, n: u32) {
        self.sketches[self.head].add_n(key, n);
    }

    /// Window-wide estimate: sum across all slices. Always >= true
    /// count over the window.
    pub fn estimate(&self, key: &str) -> u32 {
        let mut total: u32 = 0;
        for s in &self.sketches {
            total = total.saturating_add(s.estimate(key));
        }
        total
    }

    /// Estimate restricted to the current (head) slice.
    pub fn estimate_current(&self, key: &str) -> u32 {
        self.sketches[self.head].estimate(key)
    }

    /// Advance the ring: the slice immediately behind `head` becomes
    /// the new head and is cleared. The previously-oldest slice is
    /// the one that gets overwritten.
    pub fn tick(&mut self) {
        let n = self.sketches.len();
        self.head = (self.head + 1) % n;
        self.sketches[self.head].clear();
    }

    /// Drop the whole window and start from an empty ring.
    pub fn clear(&mut self) {
        for s in self.sketches.iter_mut() {
            s.clear();
        }
        self.head = 0;
    }
}

#[cfg(test)]
#[path = "windowed_tests.rs"]
mod tests;
