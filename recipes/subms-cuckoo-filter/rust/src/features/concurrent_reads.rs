//! Immutable read-side snapshot of a cuckoo filter. The base filter
//! is single-writer; this feature lets readers fan out across threads
//! against a frozen `Arc<CuckooSnapshot>` while a writer keeps
//! mutating the source filter independently.
//!
//! Snapshots are eager copies, not lazy views: taking one walks the
//! source filter's buckets once (O(N) bytes) and stores the result.
//! That keeps `contains` lock-free without any reader-side coordination,
//! at the cost of staleness vs the live writer.
//!
//! Typical use: a hot-path reader cluster shares one
//! `Arc<CuckooSnapshot>`; a background task periodically rebuilds the
//! snapshot from the live filter and swaps the `Arc` (via
//! `ArcSwap`, `RwLock<Arc<_>>`, or similar). Each reader sees a
//! consistent point-in-time view without blocking writes.

use std::sync::Arc;

use crate::{BUCKET_SIZE, CuckooFilter, alt_index_of_fp, fnv1a64, mix};

pub struct CuckooSnapshot {
    buckets: Box<[[u8; BUCKET_SIZE]]>,
    mask: usize,
    count: usize,
}

impl CuckooSnapshot {
    /// Take a snapshot of `source`. Allocates a single contiguous copy
    /// of the bucket array; no further reference to `source` is kept.
    pub fn capture(source: &CuckooFilter) -> Arc<Self> {
        let buckets: Box<[[u8; BUCKET_SIZE]]> = source.buckets_view().to_vec().into_boxed_slice();
        Arc::new(Self {
            buckets,
            mask: source.mask_view(),
            count: source.len(),
        })
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Probe membership against the frozen state. Same semantics as
    /// `CuckooFilter::contains` (false positives possible, no false
    /// negatives relative to the captured moment).
    pub fn contains(&self, key: &str) -> bool {
        let h = mix(fnv1a64(key.as_bytes()));
        let fp = ((h & 0xff) as u8).max(1);
        let i1 = (h >> 8) as usize & self.mask;
        let i2 = (i1 ^ alt_index_of_fp(fp)) & self.mask;
        self.buckets[i1].contains(&fp) || self.buckets[i2].contains(&fp)
    }

    /// Iterate over every non-empty fingerprint slot. Yields
    /// `(bucket_index, fingerprint)` pairs.
    pub fn iter_fingerprints(&self) -> impl Iterator<Item = (usize, u8)> + '_ {
        self.buckets
            .iter()
            .enumerate()
            .flat_map(|(i, b)| b.iter().filter(|&&fp| fp != 0).map(move |&fp| (i, fp)))
    }
}

#[cfg(test)]
#[path = "concurrent_reads_tests.rs"]
mod tests;
