//! Explicit iterators over the histogram in different orders.
//!
//! - **Linear**: every populated bucket in value order. Each step
//!   yields one bucket entry.
//! - **Logarithmic**: bucket boundaries aligned to powers of two
//!   (each step doubles the upper bound). Each step yields the sum
//!   of counts in the half-open band `[lo, hi)`.
//! - **Percentile**: yields buckets at evenly-spaced percentile
//!   thresholds. Caller picks the step size (e.g. 1.0 for 100
//!   percentiles, 0.1 for 1000).
//!
//! All three are zero-allocation after construction (they hold a
//! reference to the histogram and a small per-iterator cursor).

use crate::{HdrHistogram, value_from_index};

/// One step of histogram iteration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IterEntry {
    /// Lower bound (inclusive) of the value band.
    pub value_lo: u64,
    /// Upper bound (exclusive) of the value band. `u64::MAX` for the
    /// final open band.
    pub value_hi: u64,
    /// Count of records in this band.
    pub count: u64,
    /// Cumulative count from the start of iteration through this band.
    pub cumulative: u64,
}

/// Walks every populated bucket in value order.
pub struct HdrLinearIter<'a> {
    counters: &'a [u64],
    sub_count_bits: u32,
    high_index: usize,
    idx: usize,
    cumulative: u64,
}

impl<'a> HdrLinearIter<'a> {
    pub(crate) fn new(h: &'a HdrHistogram) -> Self {
        Self {
            counters: h.counters(),
            sub_count_bits: h.sub_count_bits(),
            high_index: h.high_index(),
            idx: 0,
            cumulative: 0,
        }
    }
}

impl<'a> Iterator for HdrLinearIter<'a> {
    type Item = IterEntry;
    fn next(&mut self) -> Option<Self::Item> {
        let bits = self.sub_count_bits;
        let end = (self.high_index + 1).min(self.counters.len());
        while self.idx < end {
            let i = self.idx;
            self.idx += 1;
            let c = self.counters[i];
            if c == 0 {
                continue;
            }
            let lo = value_from_index(i, bits);
            let hi = value_from_index(i + 1, bits);
            self.cumulative += c;
            return Some(IterEntry {
                value_lo: lo,
                value_hi: hi,
                count: c,
                cumulative: self.cumulative,
            });
        }
        None
    }
}

/// Walks the histogram in powers-of-two bands. Each step yields the
/// sum of counts in `[2^k, 2^(k+1))`.
pub struct HdrLogarithmicIter<'a> {
    counters: &'a [u64],
    sub_count_bits: u32,
    high_index: usize,
    /// Current lower bound of the band (a power of two), starting at 1.
    lo: u64,
    cumulative: u64,
    /// True once we've yielded the band that covers `high_index`.
    done: bool,
}

impl<'a> HdrLogarithmicIter<'a> {
    pub(crate) fn new(h: &'a HdrHistogram) -> Self {
        Self {
            counters: h.counters(),
            sub_count_bits: h.sub_count_bits(),
            high_index: h.high_index(),
            lo: 1,
            cumulative: 0,
            done: false,
        }
    }
}

impl<'a> Iterator for HdrLogarithmicIter<'a> {
    type Item = IterEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let bits = self.sub_count_bits;
        let hi = self.lo.saturating_mul(2);
        // Sum every populated bucket whose lower bound falls in [lo, hi).
        let mut count = 0u64;
        let end = (self.high_index + 1).min(self.counters.len());
        for i in 0..end {
            let v = value_from_index(i, bits);
            if v >= self.lo && v < hi {
                count += self.counters[i];
            }
        }
        self.cumulative += count;
        let entry = IterEntry {
            value_lo: self.lo,
            value_hi: hi,
            count,
            cumulative: self.cumulative,
        };
        // Walk past the high bucket and then stop.
        let high_val = value_from_index(self.high_index, bits);
        if hi > high_val {
            self.done = true;
        }
        self.lo = hi;
        Some(entry)
    }
}

/// Walks the histogram at evenly-spaced percentile thresholds.
/// `step_percent` of 1.0 yields ~100 entries; 0.1 yields ~1000.
pub struct HdrPercentileIter<'a> {
    counters: &'a [u64],
    sub_count_bits: u32,
    high_index: usize,
    total: u64,
    /// Step in percent (e.g. 1.0 for 1%).
    step_pct: f64,
    /// Next percentile threshold to emit, in percent.
    next_pct: f64,
    /// Cursor through the counter array.
    idx: usize,
    /// Cumulative count so far.
    cum: u64,
}

impl<'a> HdrPercentileIter<'a> {
    pub(crate) fn new(h: &'a HdrHistogram, step_percent: f64) -> Self {
        Self {
            counters: h.counters(),
            sub_count_bits: h.sub_count_bits(),
            high_index: h.high_index(),
            total: h.count(),
            step_pct: step_percent.max(f64::MIN_POSITIVE),
            next_pct: step_percent.max(f64::MIN_POSITIVE),
            idx: 0,
            cum: 0,
        }
    }
}

impl<'a> Iterator for HdrPercentileIter<'a> {
    type Item = IterEntry;
    fn next(&mut self) -> Option<Self::Item> {
        if self.total == 0 || self.next_pct > 100.0 + 1e-9 {
            return None;
        }
        let bits = self.sub_count_bits;
        let end = (self.high_index + 1).min(self.counters.len());
        let target = ((self.next_pct / 100.0) * self.total as f64) as u64;
        // Advance until cumulative count crosses target.
        while self.idx < end {
            self.cum += self.counters[self.idx];
            if self.cum >= target {
                let lo = value_from_index(self.idx, bits);
                let hi = value_from_index(self.idx + 1, bits);
                let pct_now = self.next_pct;
                self.next_pct += self.step_pct;
                // Don't advance idx here - the next percentile may
                // also land in this same bucket and should report
                // the same band.
                return Some(IterEntry {
                    value_lo: lo,
                    value_hi: hi,
                    count: self.counters[self.idx],
                    cumulative: self
                        .cum
                        .min(self.total)
                        .max((pct_now / 100.0 * self.total as f64) as u64),
                });
            }
            self.idx += 1;
        }
        None
    }
}

impl HdrHistogram {
    pub fn iter_linear(&self) -> HdrLinearIter<'_> {
        HdrLinearIter::new(self)
    }

    pub fn iter_logarithmic(&self) -> HdrLogarithmicIter<'_> {
        HdrLogarithmicIter::new(self)
    }

    pub fn iter_percentiles(&self, step_percent: f64) -> HdrPercentileIter<'_> {
        HdrPercentileIter::new(self, step_percent)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linear_visits_only_populated_buckets() {
        let mut h = HdrHistogram::new(3);
        for v in [10u64, 100, 1000] {
            h.record(v);
        }
        let entries: Vec<IterEntry> = h.iter_linear().collect();
        assert_eq!(entries.len(), 3, "exactly the three populated buckets");
        // value_lo strictly increasing.
        for w in entries.windows(2) {
            assert!(w[0].value_lo < w[1].value_lo, "value order");
        }
        // Cumulative ends at total.
        assert_eq!(entries.last().unwrap().cumulative, 3);
    }

    #[test]
    fn linear_on_empty_yields_nothing() {
        let h = HdrHistogram::new(3);
        let entries: Vec<IterEntry> = h.iter_linear().collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn logarithmic_bands_double() {
        let mut h = HdrHistogram::new(3);
        for v in [1u64, 3, 7, 15, 31, 100, 1000] {
            h.record(v);
        }
        let entries: Vec<IterEntry> = h.iter_logarithmic().collect();
        assert!(!entries.is_empty());
        // Each band's hi must equal the next band's lo (or 2*lo).
        for w in entries.windows(2) {
            assert_eq!(w[1].value_lo, w[0].value_hi, "abutting bands");
            assert_eq!(w[0].value_hi, w[0].value_lo * 2, "powers of two");
        }
        // Total counts across bands = total recorded.
        let total: u64 = entries.iter().map(|e| e.count).sum();
        assert_eq!(total, 7, "every record covered by some band");
    }

    #[test]
    fn logarithmic_covers_high_bucket() {
        let mut h = HdrHistogram::new(3);
        h.record(1);
        h.record(1_000_000);
        let entries: Vec<IterEntry> = h.iter_logarithmic().collect();
        // The last band must cover the 1M record.
        let total: u64 = entries.iter().map(|e| e.count).sum();
        assert_eq!(total, 2);
    }

    #[test]
    fn percentile_emits_roughly_step_entries() {
        let mut h = HdrHistogram::new(3);
        for v in 1u64..=1000 {
            h.record(v);
        }
        let entries: Vec<IterEntry> = h.iter_percentiles(10.0).collect();
        // 10% step on 100% coverage gives ~10 entries.
        assert!(
            (8..=12).contains(&entries.len()),
            "got {} percentile entries",
            entries.len()
        );
        // Percentile cumulative is monotonic.
        for w in entries.windows(2) {
            assert!(w[0].cumulative <= w[1].cumulative);
        }
    }

    #[test]
    fn percentile_on_empty_yields_nothing() {
        let h = HdrHistogram::new(3);
        let entries: Vec<IterEntry> = h.iter_percentiles(1.0).collect();
        assert!(entries.is_empty());
    }

    #[test]
    fn linear_and_percentile_share_population() {
        let mut h = HdrHistogram::new(3);
        for v in 1u64..=100 {
            h.record(v);
        }
        let linear_total: u64 = h.iter_linear().map(|e| e.count).sum();
        assert_eq!(linear_total, 100);
        let last_pct = h.iter_percentiles(1.0).last();
        assert!(last_pct.is_some());
    }
}
