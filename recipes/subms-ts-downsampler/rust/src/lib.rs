//! `subms-ts-downsampler` - a tiered downsampling pipeline. Push raw points
//! once; each tier (e.g. 1s, 1m, 1h) rolls them into fixed-width buckets,
//! emitting `(count, sum, min, max, last)` per bucket. A query planner then
//! answers at whatever resolution a tier provides without rescanning raw
//! data - the write-path side of multi-resolution storage.
//!
//! ```
//! use subms_ts_downsampler::TsDownsampler;
//!
//! // two tiers: 10-ns and 100-ns buckets
//! let mut d = TsDownsampler::new(&[10, 100]);
//! for ts in 0..250 { d.push(ts, ts as f64); }
//! d.flush();
//! // tier 0 closed 25 ten-ns buckets; tier 1 closed 3 hundred-ns buckets
//! assert_eq!(d.tier(0).len(), 25);
//! assert_eq!(d.tier(1).len(), 3);
//! ```

use subms_ts::TsSeries;

/// Per-bucket rollup. `last` is the most recent value seen in the bucket.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TsBucketStats {
    pub count: u32,
    pub sum: f64,
    pub min: f64,
    pub max: f64,
    pub last: f64,
}

impl TsBucketStats {
    fn open(value: f64) -> Self {
        Self {
            count: 1,
            sum: value,
            min: value,
            max: value,
            last: value,
        }
    }

    fn update(&mut self, value: f64) {
        self.count += 1;
        self.sum += value;
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.last = value;
    }

    pub fn mean(&self) -> f64 {
        if self.count == 0 {
            0.0
        } else {
            self.sum / self.count as f64
        }
    }
}

struct Tier {
    duration_ns: i64,
    means: TsSeries<f64>,              // (bucket_start, mean) per closed bucket
    closed: Vec<(i64, TsBucketStats)>, // aligned with `means`, full stats
    open_start: Option<i64>,
    open: TsBucketStats,
}

impl Tier {
    fn new(duration_ns: i64) -> Self {
        Self {
            duration_ns: duration_ns.max(1),
            means: TsSeries::new(),
            closed: Vec::new(),
            open_start: None,
            open: TsBucketStats::open(0.0),
        }
    }

    fn bucket_start(&self, ts: i64) -> i64 {
        ts.div_euclid(self.duration_ns) * self.duration_ns
    }

    fn push(&mut self, ts: i64, value: f64) {
        let start = self.bucket_start(ts);
        match self.open_start {
            Some(s) if s == start => self.open.update(value),
            Some(s) => {
                self.close(s);
                self.open_start = Some(start);
                self.open = TsBucketStats::open(value);
            }
            None => {
                self.open_start = Some(start);
                self.open = TsBucketStats::open(value);
            }
        }
    }

    fn close(&mut self, start: i64) {
        // means is non-decreasing in bucket_start; push cannot fail.
        let _ = self.means.push(start, self.open.mean());
        self.closed.push((start, self.open));
    }

    fn flush(&mut self) {
        if let Some(s) = self.open_start.take() {
            self.close(s);
        }
    }

    fn stats_at(&self, ts: i64) -> Option<TsBucketStats> {
        let start = self.bucket_start(ts);
        if self.open_start == Some(start) {
            return Some(self.open);
        }
        // closed is sorted by bucket_start.
        self.closed
            .binary_search_by_key(&start, |&(s, _)| s)
            .ok()
            .map(|i| self.closed[i].1)
    }
}

/// A stack of downsampling tiers fed by one `push`. Tier 0 is the finest.
pub struct TsDownsampler {
    tiers: Vec<Tier>,
}

impl TsDownsampler {
    /// One tier per bucket duration (nanoseconds), finest first.
    pub fn new(tier_durations_ns: &[i64]) -> Self {
        Self {
            tiers: tier_durations_ns.iter().map(|&d| Tier::new(d)).collect(),
        }
    }

    pub fn tier_count(&self) -> usize {
        self.tiers.len()
    }

    /// Feed a raw point to every tier.
    pub fn push(&mut self, ts: i64, value: f64) {
        for t in &mut self.tiers {
            t.push(ts, value);
        }
    }

    /// Close every tier's open bucket so the final partial bucket is emitted.
    pub fn flush(&mut self) {
        for t in &mut self.tiers {
            t.flush();
        }
    }

    /// The closed-bucket mean series for a tier (bucket_start -> mean).
    pub fn tier(&self, level: usize) -> &TsSeries<f64> {
        &self.tiers[level].means
    }

    pub fn tier_duration(&self, level: usize) -> i64 {
        self.tiers[level].duration_ns
    }

    /// Full bucket stats for the bucket containing `ts` at `level` (open or
    /// closed). `None` if no point has landed in that bucket.
    pub fn bucket_stats(&self, level: usize, ts: i64) -> Option<TsBucketStats> {
        self.tiers.get(level).and_then(|t| t.stats_at(ts))
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
