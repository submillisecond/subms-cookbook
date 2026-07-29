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
        let n = slices.max(2);
        let sketches = (0..n).map(|_| CountMinSketch::new(depth, width)).collect();
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

    pub fn add(&mut self, key: &str) {
        self.sketches[self.head].add(key);
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
}

#[cfg(test)]
#[path = "windowed_tests.rs"]
mod tests;
