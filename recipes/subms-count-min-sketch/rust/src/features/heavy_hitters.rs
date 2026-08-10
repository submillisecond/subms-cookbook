//! Top-K tracker driven by a CMS.
//!
//! On every `add()` we update the embedded sketch and then re-check
//! whether the key belongs in the top-K. The top set is stored as a
//! `Vec<(key, estimate)>` of size <= K, kept sorted by estimate
//! descending so reads are O(K). K is the constructor input; for
//! K <= ~32 the linear scan beats a heap on cache behaviour.
//!
//! Semantics:
//! - `estimate` returned in `top()` is the CMS estimate at insert/refresh
//!   time, NOT the live-recomputed value. Two reads of the same call to
//!   `top()` give the same numbers.
//! - A key already in the top set has its tracked estimate refreshed on
//!   every subsequent `add()`.
//! - If the new estimate ties the current K-th key's estimate, the
//!   existing entry stays (no churn).

use crate::CountMinSketch;

pub struct HeavyHitters {
    cms: CountMinSketch,
    k: usize,
    // Sorted by est descending; len <= k.
    top: Vec<HeavyEntry>,
}

#[derive(Clone)]
pub struct HeavyEntry {
    pub key: String,
    pub estimate: u32,
}

impl HeavyHitters {
    pub fn new(k: usize, depth: usize, width: usize) -> Self {
        Self::with_seed(k, depth, width, 0)
    }

    pub fn with_seed(k: usize, depth: usize, width: usize, seed: u64) -> Self {
        Self {
            cms: CountMinSketch::with_seed(depth, width, seed),
            k: k.max(1),
            top: Vec::with_capacity(k.max(1)),
        }
    }

    pub fn k(&self) -> usize {
        self.k
    }

    pub fn estimate(&self, key: &str) -> u32 {
        self.cms.estimate(key)
    }

    /// Total weight ingested, exactly.
    pub fn total(&self) -> u64 {
        self.cms.total()
    }

    /// The backing sketch, for the sizing and error introspection the
    /// tracker itself does not re-expose.
    pub fn sketch(&self) -> &CountMinSketch {
        &self.cms
    }

    /// Increment `key` and re-check the top-K.
    pub fn add(&mut self, key: &str) {
        self.add_n(key, 1);
    }

    /// Weighted increment. Ranking by notional or filled quantity rather than
    /// message count is the same code with a different weight.
    pub fn add_n(&mut self, key: &str, n: u32) {
        if n == 0 {
            return;
        }
        self.cms.add_n(key, n);
        let est = self.cms.estimate(key);
        self.update_top(key, est);
    }

    /// Current top-K snapshot, sorted by estimate descending.
    pub fn top(&self) -> &[HeavyEntry] {
        &self.top
    }

    /// Drop both the sketch and the top-K side index.
    pub fn clear(&mut self) {
        self.cms.clear();
        self.top.clear();
    }

    fn update_top(&mut self, key: &str, est: u32) {
        // Already in top? Refresh estimate, then re-sort that one entry.
        if let Some(pos) = self.top.iter().position(|e| e.key == key) {
            self.top[pos].estimate = est;
            self.resort_from(pos);
            return;
        }

        // Not yet in top. If we have room, push. Otherwise compare to
        // the current floor (last entry). A strict-greater comparison
        // avoids churn on ties.
        if self.top.len() < self.k {
            self.top.push(HeavyEntry {
                key: key.to_string(),
                estimate: est,
            });
            let last = self.top.len() - 1;
            self.resort_from(last);
        } else {
            let floor = self.top.last().expect("non-empty by len() == k branch");
            if est > floor.estimate {
                let last = self.top.len() - 1;
                self.top[last] = HeavyEntry {
                    key: key.to_string(),
                    estimate: est,
                };
                self.resort_from(last);
            }
        }
    }

    fn resort_from(&mut self, mut i: usize) {
        // Bubble the entry up until the slice is sorted by estimate desc.
        // Sorted invariant holds before this call except at index `i`.
        while i > 0 && self.top[i - 1].estimate < self.top[i].estimate {
            self.top.swap(i - 1, i);
            i -= 1;
        }
        // The entry may also need to move down (estimate dropped after
        // a refresh - can't happen given CMS monotonicity, but cheap).
        while i + 1 < self.top.len() && self.top[i + 1].estimate > self.top[i].estimate {
            self.top.swap(i, i + 1);
            i += 1;
        }
    }
}

#[cfg(test)]
#[path = "heavy_hitters_tests.rs"]
mod tests;
