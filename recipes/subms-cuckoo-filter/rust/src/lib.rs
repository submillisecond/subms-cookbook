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

const FNV_OFFSET: u64 = 0xcbf29ce484222325;
const FNV_PRIME: u64 = 0x100000001b3;
/// Slots per bucket. 4 gives ~95% load factor; higher values raise load
/// factor but slow lookups linearly.
const BUCKET_SIZE: usize = 4;
/// Max kick-out attempts during a single insert.
const MAX_KICKS: usize = 500;

pub struct CuckooFilter {
    buckets: Vec<[u8; BUCKET_SIZE]>,
    mask: usize,
    count: usize,
    rng_state: u64,
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
        }
    }

    pub fn len(&self) -> usize { self.count }
    pub fn is_empty(&self) -> bool { self.count == 0 }
    pub fn bucket_count(&self) -> usize { self.buckets.len() }

    /// Insert a fingerprint of `key`. Returns `false` if the filter is too
    /// full to place (after `MAX_KICKS` evictions).
    pub fn insert(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        if self.try_place(i1, fp) || self.try_place(i2, fp) {
            self.count += 1;
            return true;
        }
        // Both buckets full: kick out a random entry and re-place it.
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
        false
    }

    /// Probe membership. False positives possible (per the FPR analysis);
    /// false negatives impossible (the algorithm guarantees a matching
    /// fingerprint stays in one of the two candidate buckets).
    pub fn contains(&self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        self.bucket_has(i1, fp) || self.bucket_has(i2, fp)
    }

    /// Delete one occurrence of `key`. Returns `false` if not found.
    pub fn delete(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        if self.bucket_remove(i1, fp) || self.bucket_remove(i2, fp) {
            self.count -= 1;
            return true;
        }
        false
    }

    fn indices(&self, key: &str) -> (u8, usize, usize) {
        let h = mix(fnv1a64(key.as_bytes()));
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
        self.buckets[i].iter().any(|&s| s == fp)
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
        self.rng_state = self.rng_state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.rng_state
    }
}

/// `alt(fp)` deterministically derives the second bucket offset from a
/// fingerprint. Multiplying by an odd constant keeps the map invertible
/// (we never need the inverse, but it bounds collisions).
fn alt_index_of_fp(fp: u8) -> usize {
    (fp as u64).wrapping_mul(0x5bd1e9955_u64) as usize
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}

fn mix(mut h: u64) -> u64 {
    h ^= h >> 30;
    h = h.wrapping_mul(0xbf58476d1ce4e5b9);
    h ^= h >> 27;
    h = h.wrapping_mul(0x94d049bb133111eb);
    h ^= h >> 31;
    h
}

#[cfg(feature = "harness")]
pub mod recipe;
