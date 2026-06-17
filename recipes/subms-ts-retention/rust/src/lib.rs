//! `subms-ts-retention` - retention policies that prune a [`TsSeries`] in place
//! by age, point count, or approximate byte budget. Built entirely on the
//! series' own delete surface (`truncate_before` + `retain`), so it inherits
//! the chunk-rebuild cost model and adds no storage of its own.
//!
//! A policy combines its configured limits with "most restrictive wins":
//! age is applied first (drop everything older than `max_age_ns` behind the
//! latest point), then the count cap (the tighter of `max_points` and the
//! point budget implied by `max_bytes`) keeps only the newest points.
//!
//! ```
//! use subms_ts::TsSeries;
//! use subms_ts_retention::TsRetentionPolicy;
//!
//! let mut s = TsSeries::<f64>::new();
//! for i in 0..1000 { s.push(i, i as f64).unwrap(); }
//! let policy = TsRetentionPolicy::new().max_points(100);
//! let removed = policy.apply(&mut s);
//! assert_eq!(removed, 900);
//! assert_eq!(s.len(), 100);
//! assert_eq!(s.first().unwrap().ts, 900); // newest 100 kept
//! ```

use subms_ts::TsSeries;

#[cfg(feature = "harness")]
pub mod recipe;

/// On-heap footprint charged per point for the byte budget: an `i64` timestamp
/// plus an `f64`/`i64` value column cell. The SoA storage has no per-point
/// overhead beyond these two columns, so this is the honest per-point cost.
pub const BYTES_PER_POINT: usize = 16;

/// A retention policy. All limits are optional; an unset limit is not applied.
/// Build with the chained setters; apply with [`apply`](TsRetentionPolicy::apply).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TsRetentionPolicy {
    max_age_ns: Option<i64>,
    max_points: Option<usize>,
    max_bytes: Option<usize>,
}

impl TsRetentionPolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// Keep only points within `age` of the most recent timestamp.
    pub fn max_age_ns(mut self, age: i64) -> Self {
        self.max_age_ns = Some(age);
        self
    }

    /// Keep at most the newest `n` points.
    pub fn max_points(mut self, n: usize) -> Self {
        self.max_points = Some(n);
        self
    }

    /// Keep at most `bytes` worth of points (newest first), at
    /// [`BYTES_PER_POINT`] each.
    pub fn max_bytes(mut self, bytes: usize) -> Self {
        self.max_bytes = Some(bytes);
        self
    }

    /// The effective point cap from `max_points` and `max_bytes` (the tighter
    /// of the two), or `None` if neither is set.
    pub fn point_cap(&self) -> Option<usize> {
        let from_bytes = self.max_bytes.map(|b| b / BYTES_PER_POINT);
        match (self.max_points, from_bytes) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        }
    }

    /// Apply the policy to `series`, returning the number of points removed.
    /// Age is applied before the count cap. No-op when the series already fits.
    pub fn apply<T: Clone>(&self, series: &mut TsSeries<T>) -> usize {
        let mut removed = 0;

        if let Some(age) = self.max_age_ns {
            if let Some(last) = series.last() {
                // saturating so a huge age never wraps the cutoff past i64::MIN.
                removed += series.truncate_before(last.ts.saturating_sub(age));
            }
        }

        if let Some(cap) = self.point_cap() {
            let n = series.len();
            if n > cap {
                // retain walks front-to-back; keep the trailing `cap` points.
                let drop_before = n - cap;
                let mut i = 0usize;
                removed += series.retain(|_| {
                    let keep = i >= drop_before;
                    i += 1;
                    keep
                });
            }
        }

        removed
    }

    /// Apply to every series in an iterator (the per-collection case: fold the
    /// policy over a collection's series). Returns the total removed.
    pub fn apply_all<'a, T: Clone + 'a>(
        &self,
        series: impl IntoIterator<Item = &'a mut TsSeries<T>>,
    ) -> usize {
        series.into_iter().map(|s| self.apply(s)).sum()
    }
}
