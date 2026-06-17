//! `subms-tdigest` - a streaming quantile sketch (t-digest, Ted Dunning):
//! constant memory, mergeable, relative-error bounded - tightest near the
//! tails, which is exactly where p99 / p99.9 live. Add a value stream, ask
//! for any quantile or the CDF at a value, and merge per-shard sketches on a
//! coordinator. The streaming-quantile surface of the `timeseries` arc.
//!
//! This is the "merging" variant with the k1 scale function: centroids near
//! the median absorb more weight, centroids near the tails stay small.
//!
//! ```
//! use subms_tdigest::TsTDigest;
//!
//! let mut d = TsTDigest::new(100.0);
//! for i in 0..10_000 { d.add(i as f64); } // uniform 0..9999
//! let p50 = d.quantile(0.5);
//! assert!((p50 - 5_000.0).abs() < 200.0);
//! let p99 = d.quantile(0.99);
//! assert!((p99 - 9_900.0).abs() < 100.0); // tail is tighter
//! ```

use std::f64::consts::PI;

mod codec;
pub use codec::TsTDigestError;

#[derive(Copy, Clone, Debug, PartialEq)]
struct Centroid {
    mean: f64,
    weight: f64,
}

#[derive(Clone, Debug)]
pub struct TsTDigest {
    compression: f64,
    centroids: Vec<Centroid>,
    buffer: Vec<Centroid>,
    total: f64,
    min: f64,
    max: f64,
}

impl TsTDigest {
    pub fn new(compression: f64) -> Self {
        let compression = compression.max(20.0);
        Self {
            compression,
            centroids: Vec::new(),
            buffer: Vec::new(),
            total: 0.0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        }
    }

    pub fn compression(&self) -> f64 {
        self.compression
    }

    pub fn count(&self) -> f64 {
        self.total
    }

    pub fn is_empty(&self) -> bool {
        self.total == 0.0
    }

    pub fn add(&mut self, value: f64) {
        self.add_weighted(value, 1.0);
    }

    pub fn add_weighted(&mut self, value: f64, weight: f64) {
        if !value.is_finite() || weight <= 0.0 {
            return;
        }
        if value < self.min {
            self.min = value;
        }
        if value > self.max {
            self.max = value;
        }
        self.total += weight;
        self.buffer.push(Centroid {
            mean: value,
            weight,
        });
        if self.buffer.len() as f64 >= self.compression * 10.0 {
            self.process();
        }
    }

    fn buffer_cap(&self) -> usize {
        (self.compression * 10.0) as usize
    }

    /// k1 scale: maps a quantile to a position whose unit steps bound centroid
    /// size. The asin shape packs resolution into the tails.
    fn k1(&self, q: f64) -> f64 {
        (self.compression / (2.0 * PI)) * (2.0 * q - 1.0).clamp(-1.0, 1.0).asin()
    }

    /// Fold the buffer into the sorted centroid list under the size bound.
    fn process(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        let mut all = Vec::with_capacity(self.centroids.len() + self.buffer.len());
        all.append(&mut self.centroids);
        all.append(&mut self.buffer);
        all.sort_by(|a, b| {
            a.mean
                .partial_cmp(&b.mean)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let total = self.total;
        let mut out: Vec<Centroid> = Vec::with_capacity(all.len());
        let mut w_finalized = 0.0; // weight of centroids before the open one

        for c in all {
            match out.last_mut() {
                None => out.push(c),
                Some(last) => {
                    let q0 = w_finalized / total;
                    let q2 = (w_finalized + last.weight + c.weight) / total;
                    if self.k1(q2) - self.k1(q0) <= 1.0 {
                        let w = last.weight + c.weight;
                        last.mean = (last.mean * last.weight + c.mean * c.weight) / w;
                        last.weight = w;
                    } else {
                        w_finalized += last.weight;
                        out.push(c);
                    }
                }
            }
        }
        self.centroids = out;
    }

    fn ensure_processed(&mut self) {
        if !self.buffer.is_empty() {
            self.process();
        }
    }

    /// Estimate the value at quantile `q` in `[0, 1]`.
    pub fn quantile(&self, q: f64) -> f64 {
        // quantile reads need the buffer folded; do it on a scratch view so
        // &self stays honest.
        let mut scratch;
        let d = if self.buffer.is_empty() {
            self
        } else {
            scratch = self.clone();
            scratch.process();
            &scratch
        };
        d.quantile_processed(q)
    }

    fn quantile_processed(&self, q: f64) -> f64 {
        let cs = &self.centroids;
        if cs.is_empty() {
            return f64::NAN;
        }
        if cs.len() == 1 {
            return cs[0].mean;
        }
        let q = q.clamp(0.0, 1.0);
        let target = q * self.total;

        // center cumulative weight of centroid i
        let mut cum = 0.0;
        let centers: Vec<f64> = cs
            .iter()
            .map(|c| {
                let center = cum + c.weight / 2.0;
                cum += c.weight;
                center
            })
            .collect();

        if target <= centers[0] {
            // interpolate from the true min to the first centroid
            let denom = centers[0].max(1e-300);
            let frac = (target / denom).clamp(0.0, 1.0);
            return self.min + frac * (cs[0].mean - self.min);
        }
        let last = cs.len() - 1;
        if target >= centers[last] {
            let denom = (self.total - centers[last]).max(1e-300);
            let frac = ((target - centers[last]) / denom).clamp(0.0, 1.0);
            return cs[last].mean + frac * (self.max - cs[last].mean);
        }
        // body: find the span [i, i+1] containing target
        let mut i = 0;
        while i + 1 < cs.len() && centers[i + 1] < target {
            i += 1;
        }
        let span = (centers[i + 1] - centers[i]).max(1e-300);
        let frac = (target - centers[i]) / span;
        cs[i].mean + frac * (cs[i + 1].mean - cs[i].mean)
    }

    /// Estimate the CDF at `value`: the fraction of the distribution <= value.
    pub fn cdf(&self, value: f64) -> f64 {
        let mut scratch;
        let d = if self.buffer.is_empty() {
            self
        } else {
            scratch = self.clone();
            scratch.process();
            &scratch
        };
        d.cdf_processed(value)
    }

    fn cdf_processed(&self, value: f64) -> f64 {
        let cs = &self.centroids;
        if cs.is_empty() {
            return f64::NAN;
        }
        if value < self.min {
            return 0.0;
        }
        if value > self.max {
            return 1.0;
        }
        let mut cum = 0.0;
        let centers: Vec<f64> = cs
            .iter()
            .map(|c| {
                let center = cum + c.weight / 2.0;
                cum += c.weight;
                center
            })
            .collect();
        if value <= cs[0].mean {
            let denom = (cs[0].mean - self.min).max(1e-300);
            let frac = ((value - self.min) / denom).clamp(0.0, 1.0);
            return (frac * centers[0]) / self.total;
        }
        let last = cs.len() - 1;
        if value >= cs[last].mean {
            let denom = (self.max - cs[last].mean).max(1e-300);
            let frac = ((value - cs[last].mean) / denom).clamp(0.0, 1.0);
            return (centers[last] + frac * (self.total - centers[last])) / self.total;
        }
        let mut i = 0;
        while i + 1 < cs.len() && cs[i + 1].mean < value {
            i += 1;
        }
        let span = (cs[i + 1].mean - cs[i].mean).max(1e-300);
        let frac = (value - cs[i].mean) / span;
        (centers[i] + frac * (centers[i + 1] - centers[i])) / self.total
    }

    /// Merge another sketch into a new one (e.g. fold per-shard digests).
    pub fn merge(&self, other: &Self) -> Self {
        let mut out = Self::new(self.compression.max(other.compression));
        for c in self.centroids.iter().chain(self.buffer.iter()) {
            out.add_weighted(c.mean, c.weight);
        }
        for c in other.centroids.iter().chain(other.buffer.iter()) {
            out.add_weighted(c.mean, c.weight);
        }
        out.process();
        out
    }

    // ----- accessors for the codec (centroids after folding) -----

    pub(crate) fn parts(&self) -> (f64, f64, f64, &[Centroid]) {
        (self.compression, self.min, self.max, &self.centroids)
    }

    pub(crate) fn from_parts(
        compression: f64,
        min: f64,
        max: f64,
        centroids: Vec<Centroid>,
    ) -> Self {
        let total = centroids.iter().map(|c| c.weight).sum();
        Self {
            compression,
            centroids,
            buffer: Vec::new(),
            total,
            min,
            max,
        }
    }

    /// Fold any buffered points so `serialize` sees the final centroids.
    pub fn compact(&mut self) {
        self.ensure_processed();
        let _ = self.buffer_cap();
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
