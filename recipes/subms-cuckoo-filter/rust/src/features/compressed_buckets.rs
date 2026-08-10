//! Compressed-bucket cuckoo filter. Stores each bucket as a
//! variable-length run of sorted fingerprints with a 1-byte count
//! prefix, instead of the fixed 4-byte slot array of the base filter.
//!
//! Memory shape per bucket:
//!
//! ```text
//! count : u8      ; 0..=BUCKET_SIZE
//! fps   : u8 * count   (sorted ascending)
//! ```
//!
//! At 50% load (2 fps/bucket average) the on-disk footprint is ~3
//! bytes/bucket vs 4 for the base; at 95% load it's ~5 vs 4 (we lose
//! to the base when buckets stay near-full). The win is the
//! low-to-moderate load regime, which is where most production cuckoo
//! filters sit (95% is the saturation cliff, not the operating point).
//!
//! Lookup is a binary search over a sorted run of at most BUCKET_SIZE
//! bytes; insert and delete keep the run sorted via memmove. Both pay
//! O(BUCKET_SIZE) per op vs O(BUCKET_SIZE) in the base, but with a
//! constant factor of ~2 for the move.

use std::io::{self, Write};

use crate::{BUCKET_SIZE, alt_index_of_fp, fnv1a64, mix};

const MAX_KICKS: usize = 500;
const HEADER: usize = 4 + 8 + 1 + 4;

pub struct CompressedCuckooFilter {
    /// Flat backing buffer holding every bucket back-to-back. Per-bucket
    /// layout is `(count_byte, fp0, fp1, ...)` with `count_byte` slots
    /// of capacity. We pre-allocate the worst-case width per bucket so
    /// inserts don't need to shift the entire tail.
    buckets: Vec<[u8; BUCKET_SIZE + 1]>,
    mask: usize,
    count: usize,
    rng_state: u64,
    /// Same parked-victim slot as the base filter.
    victim_fp: u8,
    victim_bucket: usize,
}

impl CompressedCuckooFilter {
    pub fn with_capacity(expected_entries: usize) -> Self {
        let needed = (expected_entries.max(1) * 105 / 100) / BUCKET_SIZE + 1;
        let num_buckets = needed.max(2).next_power_of_two();
        Self {
            buckets: vec![[0u8; BUCKET_SIZE + 1]; num_buckets],
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

    /// Byte cost of the live (occupied) state. Excludes the
    /// pre-allocated padding; this is the wire-format size.
    pub fn occupied_bytes(&self) -> usize {
        self.buckets.iter().map(|b| 1 + b[0] as usize).sum()
    }

    /// Serialise in the compact form: the same 17-byte header as the base
    /// filter, then each bucket as a count byte followed by exactly that many
    /// fingerprints. The empty tail slots the base layout always writes are
    /// what this feature exists to leave out, so the stream is
    /// [`Self::occupied_bytes`] plus the header. Byte-identical to the Java
    /// port's `writeTo`.
    pub fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&(self.buckets.len() as u32).to_be_bytes())?;
        out.write_all(&(self.count as u64).to_be_bytes())?;
        out.write_all(&[self.victim_fp])?;
        out.write_all(&(self.victim_bucket as u32).to_be_bytes())?;
        for b in &self.buckets {
            let n = b[0] as usize;
            out.write_all(&b[..=n])?;
        }
        Ok(())
    }

    /// Parse a compact-form filter. Each bucket's count byte is validated
    /// before its run is read, because a corrupt count would otherwise walk
    /// the cursor off the end of the buffer.
    pub fn parse(buf: &[u8]) -> io::Result<Self> {
        let invalid = |m: &'static str| io::Error::new(io::ErrorKind::InvalidData, m);
        if buf.len() < HEADER {
            return Err(invalid("compressed cuckoo header too short"));
        }
        let num_buckets = u32::from_be_bytes(buf[0..4].try_into().unwrap()) as usize;
        let count = u64::from_be_bytes(buf[4..12].try_into().unwrap()) as usize;
        let victim_fp = buf[12];
        let victim_bucket = u32::from_be_bytes(buf[13..17].try_into().unwrap()) as usize;
        if num_buckets < 2 || !num_buckets.is_power_of_two() {
            return Err(invalid("bucket count must be a power of two >= 2"));
        }
        if victim_bucket >= num_buckets {
            return Err(invalid("victim bucket out of range"));
        }
        let mut buckets = Vec::with_capacity(num_buckets);
        let mut off = HEADER;
        for _ in 0..num_buckets {
            if off >= buf.len() {
                return Err(invalid("compressed cuckoo body truncated"));
            }
            let n = buf[off] as usize;
            if n > BUCKET_SIZE || off + 1 + n > buf.len() {
                return Err(invalid("compressed cuckoo bucket run out of range"));
            }
            let mut b = [0u8; BUCKET_SIZE + 1];
            b[0] = n as u8;
            b[1..=n].copy_from_slice(&buf[off + 1..off + 1 + n]);
            buckets.push(b);
            off += 1 + n;
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

    pub fn clear(&mut self) {
        self.buckets.fill([0u8; BUCKET_SIZE + 1]);
        self.count = 0;
        self.victim_fp = 0;
        self.victim_bucket = 0;
    }

    pub fn insert(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
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
            victim = self.swap_at(bucket_idx, slot, victim);
            bucket_idx ^= alt_index_of_fp(victim) & self.mask;
            if self.try_place(bucket_idx, victim) {
                self.count += 1;
                return true;
            }
        }
        self.victim_fp = victim;
        self.victim_bucket = bucket_idx;
        self.count += 1;
        true
    }

    pub fn contains(&self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        self.bucket_has(i1, fp) || self.bucket_has(i2, fp) || self.victim_matches(fp, i1, i2)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
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

    fn victim_matches(&self, fp: u8, i1: usize, i2: usize) -> bool {
        self.victim_fp == fp && (self.victim_bucket == i1 || self.victim_bucket == i2)
    }

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

    fn indices(&self, key: &str) -> (u8, usize, usize) {
        let h = mix(fnv1a64(key.as_bytes()));
        let fp = ((h & 0xff) as u8).max(1);
        let i1 = (h >> 8) as usize & self.mask;
        let i2 = (i1 ^ alt_index_of_fp(fp)) & self.mask;
        (fp, i1, i2)
    }

    fn try_place(&mut self, i: usize, fp: u8) -> bool {
        let count = self.buckets[i][0] as usize;
        if count >= BUCKET_SIZE {
            return false;
        }
        // Insert fp keeping the run sorted ascending. Linear scan over
        // at most BUCKET_SIZE bytes; no allocation.
        let mut pos = 0;
        while pos < count && self.buckets[i][1 + pos] < fp {
            pos += 1;
        }
        // Shift tail one byte to the right, then write fp at the gap.
        for k in (pos..count).rev() {
            self.buckets[i][1 + k + 1] = self.buckets[i][1 + k];
        }
        self.buckets[i][1 + pos] = fp;
        self.buckets[i][0] = (count + 1) as u8;
        true
    }

    fn bucket_has(&self, i: usize, fp: u8) -> bool {
        let count = self.buckets[i][0] as usize;
        // Linear scan beats binary search at BUCKET_SIZE=4 (branch
        // predictor + SIMD-friendly). Still O(B) which is what we want.
        for k in 0..count {
            let cur = self.buckets[i][1 + k];
            if cur == fp {
                return true;
            }
            if cur > fp {
                return false;
            }
        }
        false
    }

    fn bucket_remove(&mut self, i: usize, fp: u8) -> bool {
        let count = self.buckets[i][0] as usize;
        for k in 0..count {
            if self.buckets[i][1 + k] == fp {
                // Shift the rest one byte left.
                for j in k..count - 1 {
                    self.buckets[i][1 + j] = self.buckets[i][1 + j + 1];
                }
                self.buckets[i][1 + count - 1] = 0;
                self.buckets[i][0] = (count - 1) as u8;
                return true;
            }
        }
        false
    }

    /// Swap a fingerprint at slot index `slot` inside bucket `i` and
    /// return the evicted fingerprint. Maintains the sorted invariant
    /// after the swap by removing the old value and re-inserting `fp`.
    fn swap_at(&mut self, i: usize, slot: usize, fp: u8) -> u8 {
        let count = self.buckets[i][0] as usize;
        let s = slot.min(count.saturating_sub(1));
        let victim = self.buckets[i][1 + s];
        // Remove victim, then re-insert fp via the sorted-insert path
        // so the bucket stays well-formed.
        for j in s..count - 1 {
            self.buckets[i][1 + j] = self.buckets[i][1 + j + 1];
        }
        self.buckets[i][1 + count - 1] = 0;
        self.buckets[i][0] = (count - 1) as u8;
        let placed = self.try_place(i, fp);
        debug_assert!(placed, "swap_at removed a slot but couldn't reinsert");
        victim
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

#[cfg(test)]
#[path = "compressed_buckets_tests.rs"]
mod tests;
