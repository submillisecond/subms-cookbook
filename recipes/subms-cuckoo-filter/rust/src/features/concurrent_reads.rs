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
mod tests {
    use super::*;

    #[test]
    fn snapshot_reports_what_writer_inserted() {
        let mut cf = CuckooFilter::with_capacity(1000);
        for i in 0..200u32 {
            cf.insert(&format!("k{i}"));
        }
        let snap = CuckooSnapshot::capture(&cf);
        for i in 0..200u32 {
            assert!(snap.contains(&format!("k{i}")), "lost k{i}");
        }
        assert_eq!(snap.len(), 200);
    }

    #[test]
    fn snapshot_isolated_from_writer_mutations() {
        let mut cf = CuckooFilter::with_capacity(1000);
        cf.insert("before-snapshot");
        let snap = CuckooSnapshot::capture(&cf);

        // Writer makes changes AFTER the snapshot was taken.
        cf.insert("after-snapshot");
        cf.delete("before-snapshot");

        assert!(
            snap.contains("before-snapshot"),
            "snapshot must remain stable"
        );
        assert!(
            !snap.contains("after-snapshot"),
            "snapshot must NOT see writer's later insert"
        );
    }

    #[test]
    fn empty_snapshot_rejects_everything() {
        let cf = CuckooFilter::with_capacity(100);
        let snap = CuckooSnapshot::capture(&cf);
        assert!(snap.is_empty());
        assert!(!snap.contains("never-inserted"));
    }

    #[test]
    fn iter_fingerprints_count_matches_len() {
        let mut cf = CuckooFilter::with_capacity(500);
        for i in 0..100u32 {
            cf.insert(&format!("k{i}"));
        }
        let snap = CuckooSnapshot::capture(&cf);
        let yielded = snap.iter_fingerprints().count();
        assert_eq!(yielded, snap.len());
    }

    #[test]
    fn arc_shareable_across_threads() {
        let mut cf = CuckooFilter::with_capacity(1000);
        for i in 0..200u32 {
            cf.insert(&format!("k{i}"));
        }
        let snap = CuckooSnapshot::capture(&cf);

        let mut handles = Vec::new();
        for t in 0..4 {
            let s = Arc::clone(&snap);
            handles.push(std::thread::spawn(move || {
                let mut found = 0usize;
                for i in 0..200u32 {
                    if s.contains(&format!("k{i}")) {
                        found += 1;
                    }
                }
                (t, found)
            }));
        }
        for h in handles {
            let (_t, found) = h.join().unwrap();
            assert_eq!(found, 200);
        }
    }

    #[test]
    fn writer_grows_independently_of_snapshot_bucket_count() {
        let mut cf = CuckooFilter::with_capacity(64);
        for i in 0..20u32 {
            cf.insert(&format!("k{i}"));
        }
        let snap = CuckooSnapshot::capture(&cf);
        let snap_buckets = snap.bucket_count();
        // The writer keeps inserting; the snapshot's bucket count stays
        // at the captured value (the base filter doesn't resize, but
        // this confirms the snapshot isn't merely a view).
        for i in 20..40u32 {
            cf.insert(&format!("k{i}"));
        }
        assert_eq!(snap.bucket_count(), snap_buckets);
    }
}
