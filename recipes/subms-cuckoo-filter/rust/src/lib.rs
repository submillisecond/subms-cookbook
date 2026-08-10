//! Cuckoo filter. Bloom-alternative that supports delete.
//!
//! Two candidate buckets per key: `i1 = h(key) & mask`, `i2 = i1 ^ h(fp) & mask`.
//! Each bucket holds `B` 8-bit fingerprints. Insert tries i1 then i2; if both
//! full, kicks a random fingerprint out and re-places it. Delete removes a
//! matching fingerprint from either bucket.
//!
//! ```
//! use subms_cuckoo_filter::CuckooFilter;
//! let mut cf = CuckooFilter::with_capacity(10_000);
//! assert!(cf.insert("hello"));
//! assert!(cf.contains("hello"));
//! assert!(cf.delete("hello"));
//! assert!(!cf.contains("hello"));
//! ```
//!
//! Single writer. `CuckooFilter` is `Send + Sync` in the ordinary Rust sense
//! (`&mut` for every mutation), and has no internal synchronisation: two
//! threads mutating one filter is a compile error, and shared read access
//! while a writer holds `&mut` is too. For read fan-out across threads take a
//! [`CuckooSnapshot`] behind the `concurrent-reads` feature.
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-cuckoo-filter>

use std::io::{self, Write};

pub(crate) const FNV_OFFSET: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;
/// Slots per bucket. 4 gives ~95% load factor; higher values raise load
/// factor but slow lookups linearly.
pub const BUCKET_SIZE: usize = 4;
/// Max kick-out attempts during a single insert.
pub const MAX_KICKS: usize = 500;
/// Fingerprint bits held per slot in the base filter. The `variable-fingerprint`
/// feature widens this to 12 or 16.
pub const FINGERPRINT_BITS: u32 = 8;

/// Every way an operation on a [`CuckooFilter`] can refuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CuckooError {
    /// The eviction chain hit [`MAX_KICKS`] and the victim slot was already
    /// occupied, so there is nowhere left to put a fingerprint. Size the
    /// filter larger, or reach for the `dynamic` feature.
    NotEnoughSpace,
    /// [`CuckooFilter::union`] was handed a filter with a different bucket
    /// count. Bucket `i` of one filter has no relationship to bucket `i` of
    /// the other unless the geometries match.
    GeometryMismatch { lhs: usize, rhs: usize },
}

impl std::fmt::Display for CuckooError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CuckooError::NotEnoughSpace => write!(f, "cuckoo filter is saturated"),
            CuckooError::GeometryMismatch { lhs, rhs } => {
                write!(f, "incompatible cuckoo geometry: {lhs} buckets vs {rhs}")
            }
        }
    }
}

impl std::error::Error for CuckooError {}

pub struct CuckooFilter {
    buckets: Vec<[u8; BUCKET_SIZE]>,
    mask: usize,
    count: usize,
    rng_state: u64,
    /// The one fingerprint the eviction chain could not re-home, held here
    /// rather than dropped. Without it a saturating insert silently evicts an
    /// already-present key and the no-false-negative guarantee breaks. Zero
    /// means empty, matching the slot sentinel.
    victim_fp: u8,
    victim_bucket: usize,
}

impl CuckooFilter {
    /// Sized for `expected_entries` at ~95% load. Bucket count rounded up to
    /// a power of two.
    pub fn with_capacity(expected_entries: usize) -> Self {
        let needed = (expected_entries.max(1) * 105 / 100) / BUCKET_SIZE + 1;
        let num_buckets = needed.max(2).next_power_of_two();
        Self {
            buckets: vec![[0u8; BUCKET_SIZE]; num_buckets],
            mask: num_buckets - 1,
            count: 0,
            rng_state: 0x9e3779b97f4a7c15,
            victim_fp: 0,
            victim_bucket: 0,
        }
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

    /// Total fingerprint slots. The filter refuses new keys somewhere below
    /// this, around 95% occupancy at `BUCKET_SIZE = 4`.
    pub fn capacity(&self) -> usize {
        self.buckets.len() * BUCKET_SIZE
    }

    /// Occupied fraction of [`Self::capacity`]. The number that decides
    /// whether inserts are about to start failing.
    pub fn load_factor(&self) -> f64 {
        self.count as f64 / self.capacity() as f64
    }

    /// Bytes held by the bucket array. Excludes the `Vec` header and the
    /// handful of scalar fields; this is the term that scales.
    pub fn size_in_bytes(&self) -> usize {
        self.buckets.len() * BUCKET_SIZE
    }

    /// False-positive probability at the current occupancy:
    /// `1 - (1 - 2^-f)^(2 * b * alpha)` for `f` fingerprint bits, `b` slots
    /// per bucket and load factor `alpha`. A query touches `2b` slots, each a
    /// `2^-f` chance of a fingerprint collision. Empty filter reports zero.
    pub fn estimated_fpp(&self) -> f64 {
        let alpha = self.load_factor();
        if alpha <= 0.0 {
            return 0.0;
        }
        let per_slot = 1.0 - 2f64.powi(-(FINGERPRINT_BITS as i32));
        1.0 - per_slot.powf(2.0 * BUCKET_SIZE as f64 * alpha)
    }

    /// Zero every slot, keeping the allocation. A session boundary that
    /// rebuilds membership from a source of truth reuses the array instead of
    /// dropping and re-allocating it.
    pub fn clear(&mut self) {
        self.buckets.fill([0u8; BUCKET_SIZE]);
        self.count = 0;
        self.victim_fp = 0;
        self.victim_bucket = 0;
    }

    /// Insert a fingerprint of `key`. Returns `false` if the filter is too
    /// full to place (after `MAX_KICKS` evictions with the victim slot
    /// already spoken for).
    pub fn insert(&mut self, key: &str) -> bool {
        self.insert_bytes(key.as_bytes())
    }

    /// Insert over raw bytes. Market-data keys are rarely `String` - an order
    /// id is a `u64`, a symbol is a fixed-width field off the wire - and
    /// forcing a UTF-8 allocation to reach the filter would dominate the op.
    pub fn insert_bytes(&mut self, key: &[u8]) -> bool {
        let (fp, i1, i2) = self.indices_bytes(key);
        self.place(fp, i1, i2)
    }

    /// Insert only if the key is not already present, returning `true` when it
    /// was added. One probe instead of two for the dedup shape: a feed handler
    /// asking "have I seen this sequence number" and recording it in the same
    /// breath. A false positive suppresses a genuinely new key, which is the
    /// trade a dedup window is making anyway.
    pub fn insert_if_absent(&mut self, key: &str) -> bool {
        self.insert_if_absent_bytes(key.as_bytes())
    }

    pub fn insert_if_absent_bytes(&mut self, key: &[u8]) -> bool {
        let (fp, i1, i2) = self.indices_bytes(key);
        if self.bucket_has(i1, fp) || self.bucket_has(i2, fp) || self.victim_matches(fp, i1, i2) {
            return false;
        }
        self.place(fp, i1, i2)
    }

    /// [`Self::insert`] with a typed refusal instead of a bare `bool`.
    pub fn try_insert(&mut self, key: &str) -> Result<(), CuckooError> {
        if self.insert(key) {
            Ok(())
        } else {
            Err(CuckooError::NotEnoughSpace)
        }
    }

    /// Probe membership. False positives possible (per the FPR analysis);
    /// false negatives impossible - every cuckoo move leaves a fingerprint in
    /// one of its two candidate buckets, and the one fingerprint an
    /// oversubscribed insert cannot re-home is held in the victim slot rather
    /// than dropped.
    pub fn contains(&self, key: &str) -> bool {
        self.contains_bytes(key.as_bytes())
    }

    pub fn contains_bytes(&self, key: &[u8]) -> bool {
        let (fp, i1, i2) = self.indices_bytes(key);
        self.bucket_has(i1, fp) || self.bucket_has(i2, fp) || self.victim_matches(fp, i1, i2)
    }

    /// Delete one occurrence of `key`. Returns `false` if not found.
    pub fn delete(&mut self, key: &str) -> bool {
        self.delete_bytes(key.as_bytes())
    }

    pub fn delete_bytes(&mut self, key: &[u8]) -> bool {
        let (fp, i1, i2) = self.indices_bytes(key);
        if self.bucket_remove(i1, fp) || self.bucket_remove(i2, fp) {
            self.count -= 1;
            self.rehome_victim();
            return true;
        }
        if self.victim_matches(fp, i1, i2) {
            self.victim_fp = 0;
            self.count -= 1;
            return true;
        }
        false
    }

    /// Merge `other` into this filter. Both must have the same bucket count:
    /// a fingerprint's home bucket is an index into a specific geometry, so
    /// copying one across filters of different widths would land it somewhere
    /// neither candidate bucket covers, which is a false negative.
    ///
    /// Unlike a bloom filter's OR, this walks and re-places every fingerprint,
    /// so it is O(N) in `other`'s capacity and can fail on saturation. A
    /// failed merge leaves the fingerprints placed so far in place; rebuild
    /// from the sources rather than retrying into the same filter.
    pub fn union(&mut self, other: &CuckooFilter) -> Result<(), CuckooError> {
        if self.buckets.len() != other.buckets.len() {
            return Err(CuckooError::GeometryMismatch {
                lhs: self.buckets.len(),
                rhs: other.buckets.len(),
            });
        }
        for (i, bucket) in other.buckets.iter().enumerate() {
            for &fp in bucket {
                if fp != 0 && !self.place(fp, i, (i ^ alt_index_of_fp(fp)) & self.mask) {
                    return Err(CuckooError::NotEnoughSpace);
                }
            }
        }
        if other.victim_fp != 0 {
            let i1 = other.victim_bucket;
            let i2 = (i1 ^ alt_index_of_fp(other.victim_fp)) & self.mask;
            if !self.place(other.victim_fp, i1, i2) {
                return Err(CuckooError::NotEnoughSpace);
            }
        }
        Ok(())
    }

    /// Serialise to the cross-language wire format: bucket count, live count
    /// and victim slot as big-endian headers, then the bucket bytes in index
    /// order. The Java port reads and writes the same bytes. The PRNG state
    /// is deliberately not carried - it only picks which slot to evict, so a
    /// reloaded filter answers every query identically.
    pub fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&(self.buckets.len() as u32).to_be_bytes())?;
        out.write_all(&(self.count as u64).to_be_bytes())?;
        out.write_all(&[self.victim_fp])?;
        out.write_all(&(self.victim_bucket as u32).to_be_bytes())?;
        for b in &self.buckets {
            out.write_all(b)?;
        }
        Ok(())
    }

    /// Parse a serialised filter. Rejects a truncated buffer and a bucket
    /// count that is not a power of two - the mask arithmetic is only a
    /// modular reduction when it is, and a corrupt header would otherwise
    /// index out of the array on the first probe.
    pub fn parse(buf: &[u8]) -> io::Result<Self> {
        const HEADER: usize = 4 + 8 + 1 + 4;
        if buf.len() < HEADER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cuckoo header too short",
            ));
        }
        let num_buckets = u32::from_be_bytes(buf[0..4].try_into().unwrap()) as usize;
        let count = u64::from_be_bytes(buf[4..12].try_into().unwrap()) as usize;
        let victim_fp = buf[12];
        let victim_bucket = u32::from_be_bytes(buf[13..17].try_into().unwrap()) as usize;
        if num_buckets < 2 || !num_buckets.is_power_of_two() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cuckoo bucket count must be a power of two >= 2",
            ));
        }
        if buf.len() < HEADER + num_buckets * BUCKET_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cuckoo body truncated",
            ));
        }
        if victim_bucket >= num_buckets {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "cuckoo victim bucket out of range",
            ));
        }
        let mut buckets = Vec::with_capacity(num_buckets);
        for i in 0..num_buckets {
            let off = HEADER + i * BUCKET_SIZE;
            let mut b = [0u8; BUCKET_SIZE];
            b.copy_from_slice(&buf[off..off + BUCKET_SIZE]);
            buckets.push(b);
        }
        Ok(Self {
            buckets,
            mask: num_buckets - 1,
            count,
            rng_state: 0x9e3779b97f4a7c15,
            victim_fp,
            victim_bucket,
        })
    }

    fn place(&mut self, fp: u8, i1: usize, i2: usize) -> bool {
        if self.try_place(i1, fp) || self.try_place(i2, fp) {
            self.count += 1;
            return true;
        }
        if self.victim_fp != 0 {
            return false;
        }
        let mut bucket_idx = if self.rand_bit() { i1 } else { i2 };
        let mut victim = fp;
        for _ in 0..MAX_KICKS {
            let slot = (self.next_random() as usize) % BUCKET_SIZE;
            std::mem::swap(&mut victim, &mut self.buckets[bucket_idx][slot]);
            bucket_idx ^= alt_index_of_fp(victim) & self.mask;
            if self.try_place(bucket_idx, victim) {
                self.count += 1;
                return true;
            }
        }
        // The chain ran out of moves holding a fingerprint that is already
        // part of the set. Park it instead of dropping it.
        self.victim_fp = victim;
        self.victim_bucket = bucket_idx;
        self.count += 1;
        true
    }

    fn victim_matches(&self, fp: u8, i1: usize, i2: usize) -> bool {
        self.victim_fp == fp && (self.victim_bucket == i1 || self.victim_bucket == i2)
    }

    /// A delete frees a slot, so the parked fingerprint may fit again. Try it
    /// on the way out of every successful delete: leaving the victim set is
    /// what turns the next insert into a refusal.
    fn rehome_victim(&mut self) {
        if self.victim_fp == 0 {
            return;
        }
        let fp = self.victim_fp;
        let alt = (self.victim_bucket ^ alt_index_of_fp(fp)) & self.mask;
        if self.try_place(self.victim_bucket, fp) || self.try_place(alt, fp) {
            self.victim_fp = 0;
        }
    }

    fn indices_bytes(&self, key: &[u8]) -> (u8, usize, usize) {
        let h = mix(fnv1a64(key));
        // Use the low byte as the 8-bit fingerprint. Avoid fp == 0 (we use 0
        // to mark empty slots).
        let fp = ((h & 0xff) as u8).max(1);
        let i1 = (h >> 8) as usize & self.mask;
        let i2 = (i1 ^ alt_index_of_fp(fp)) & self.mask;
        (fp, i1, i2)
    }

    fn try_place(&mut self, i: usize, fp: u8) -> bool {
        for slot in &mut self.buckets[i] {
            if *slot == 0 {
                *slot = fp;
                return true;
            }
        }
        false
    }

    fn bucket_has(&self, i: usize, fp: u8) -> bool {
        self.buckets[i].contains(&fp)
    }

    fn bucket_remove(&mut self, i: usize, fp: u8) -> bool {
        for slot in &mut self.buckets[i] {
            if *slot == fp {
                *slot = 0;
                return true;
            }
        }
        false
    }

    fn rand_bit(&mut self) -> bool {
        self.next_random() & 1 != 0
    }

    fn next_random(&mut self) -> u64 {
        self.rng_state = self
            .rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng_state
    }
}

#[cfg(feature = "concurrent-reads")]
impl CuckooFilter {
    /// Borrow the raw bucket array. Used by the snapshot to traverse without
    /// copying twice.
    pub(crate) fn buckets_view(&self) -> &[[u8; BUCKET_SIZE]] {
        &self.buckets
    }

    pub(crate) fn mask_view(&self) -> usize {
        self.mask
    }

    /// `(fingerprint, bucket)` of the parked eviction victim, or `(0, 0)`.
    pub(crate) fn victim_view(&self) -> (u8, usize) {
        (self.victim_fp, self.victim_bucket)
    }
}

/// `alt(fp)` deterministically derives the second bucket offset from a
/// fingerprint. Multiplying by an odd constant keeps the map invertible
/// (we never need the inverse, but it bounds collisions).
pub(crate) fn alt_index_of_fp(fp: u8) -> usize {
    (fp as u64).wrapping_mul(0x5bd1e9955_u64) as usize
}

pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

pub(crate) fn mix(mut h: u64) -> u64 {
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    h
}

#[cfg(test)]
#[path = "cuckoo_tests.rs"]
mod cuckoo_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each submodule is gated by its own Cargo
// feature; the base filter stays zero-dep + std-only.
#[cfg(any(
    feature = "variable-fingerprint",
    feature = "dynamic",
    feature = "concurrent-reads",
    feature = "compressed-buckets",
))]
pub mod features;

#[cfg(feature = "compressed-buckets")]
pub use features::compressed_buckets::CompressedCuckooFilter;
#[cfg(feature = "concurrent-reads")]
pub use features::concurrent_reads::CuckooSnapshot;
#[cfg(feature = "dynamic")]
pub use features::dynamic::DynamicCuckooFilter;
#[cfg(feature = "variable-fingerprint")]
pub use features::variable_fingerprint::{FingerprintWidth, VariableFpCuckooFilter};
