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
        Self {
            cms: CountMinSketch::new(depth, width),
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

    /// Increment `key` and re-check the top-K.
    pub fn add(&mut self, key: &str) {
        self.cms.add(key);
        let est = self.cms.estimate(key);
        self.update_top(key, est);
    }

    /// Current top-K snapshot, sorted by estimate descending.
    pub fn top(&self) -> &[HeavyEntry] {
        &self.top
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
mod tests {
    use super::*;

    #[test]
    fn empty_top_is_empty() {
        let hh = HeavyHitters::new(5, 5, 1024);
        assert!(hh.top().is_empty());
    }

    #[test]
    fn fewer_than_k_distinct_keys_all_present() {
        let mut hh = HeavyHitters::new(10, 5, 1024);
        for _ in 0..100 {
            hh.add("a");
        }
        for _ in 0..50 {
            hh.add("b");
        }
        for _ in 0..25 {
            hh.add("c");
        }
        let top = hh.top();
        assert_eq!(top.len(), 3);
        // Descending order by estimate.
        assert_eq!(top[0].key, "a");
        assert_eq!(top[1].key, "b");
        assert_eq!(top[2].key, "c");
        assert!(top[0].estimate >= top[1].estimate);
        assert!(top[1].estimate >= top[2].estimate);
    }

    #[test]
    fn cold_keys_evicted_when_hotter_arrive() {
        let mut hh = HeavyHitters::new(2, 5, 1024);
        for _ in 0..10 {
            hh.add("cold");
        }
        for _ in 0..20 {
            hh.add("warm");
        }
        // Both fit. Now add a hot key that should evict "cold".
        for _ in 0..100 {
            hh.add("hot");
        }
        let top = hh.top();
        assert_eq!(top.len(), 2);
        let keys: Vec<&str> = top.iter().map(|e| e.key.as_str()).collect();
        assert!(keys.contains(&"hot"));
        assert!(keys.contains(&"warm"));
        assert!(!keys.contains(&"cold"));
        // First entry is the hottest.
        assert_eq!(top[0].key, "hot");
    }

    #[test]
    fn existing_top_key_refreshed_in_place() {
        let mut hh = HeavyHitters::new(3, 5, 1024);
        hh.add("a");
        hh.add("b");
        hh.add("c");
        // Bump "c" past "a" and "b".
        for _ in 0..50 {
            hh.add("c");
        }
        let top = hh.top();
        assert_eq!(top.len(), 3);
        assert_eq!(top[0].key, "c");
        assert!(top[0].estimate >= 50);
    }

    #[test]
    fn k_one_tracks_only_hottest() {
        let mut hh = HeavyHitters::new(1, 5, 1024);
        for _ in 0..3 {
            hh.add("low");
        }
        for _ in 0..10 {
            hh.add("high");
        }
        let top = hh.top();
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].key, "high");
    }

    #[test]
    fn ties_do_not_churn_existing_entries() {
        // Equal counts: the first occupant of the slot wins.
        let mut hh = HeavyHitters::new(2, 5, 1024);
        for _ in 0..5 {
            hh.add("first");
        }
        for _ in 0..5 {
            hh.add("second");
        }
        // Now "third" reaches the same count as the floor entry.
        for _ in 0..5 {
            hh.add("third");
        }
        let top = hh.top();
        let keys: Vec<&str> = top.iter().map(|e| e.key.as_str()).collect();
        // "third" arrived after the slots were full at est=5, so it
        // should NOT have evicted either incumbent on the strict-greater rule.
        assert!(keys.contains(&"first"));
        assert!(keys.contains(&"second"));
        assert!(!keys.contains(&"third"));
    }

    #[test]
    fn k_floor_enforced() {
        // k=0 should be clamped to 1 (otherwise the top set is useless).
        let hh = HeavyHitters::new(0, 5, 1024);
        assert_eq!(hh.k(), 1);
    }
}
