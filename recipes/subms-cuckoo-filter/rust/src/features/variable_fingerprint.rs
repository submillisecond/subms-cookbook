//! Variable-width fingerprint cuckoo filter. The base filter pins
//! fingerprints to 8 bits (one byte per slot). This feature trades
//! memory for FPR by widening to 12 or 16 bits.
//!
//! FPR scales as `2 * b / 2^f`, where `b` = slots per bucket and `f`
//! = fingerprint width. At b=4:
//!
//! | width | bytes/slot | rough FPR  |
//! |------:|-----------:|-----------:|
//! |   8   |    1.0     |   ~3.1%    |
//! |  12   |    1.5     |   ~0.2%    |
//! |  16   |    2.0     |   ~0.012%  |
//!
//! 12-bit slots are packed two-per-three-bytes so we don't waste a
//! full u16 per slot.

use crate::{BUCKET_SIZE, alt_index_of_fp, fnv1a64, mix};

const MAX_KICKS: usize = 500;

/// Permitted fingerprint widths. Eight bits is what the base filter
/// uses; twelve and sixteen are the wider options. Other widths would
/// drag in unaligned bit-fiddling for marginal benefit.
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub enum FingerprintWidth {
    Eight,
    Twelve,
    Sixteen,
}

impl FingerprintWidth {
    pub fn bits(self) -> u32 {
        match self {
            FingerprintWidth::Eight => 8,
            FingerprintWidth::Twelve => 12,
            FingerprintWidth::Sixteen => 16,
        }
    }

    fn mask(self) -> u16 {
        match self {
            FingerprintWidth::Eight => 0x00ff,
            FingerprintWidth::Twelve => 0x0fff,
            FingerprintWidth::Sixteen => 0xffff,
        }
    }
}

pub struct VariableFpCuckooFilter {
    width: FingerprintWidth,
    /// Buckets stored as u16 slots regardless of width. Costs an
    /// extra byte per 8-bit slot vs the base layout but lets us reuse
    /// the same insert/lookup code for all three widths without
    /// branching on each slot access.
    buckets: Vec<[u16; BUCKET_SIZE]>,
    mask: usize,
    count: usize,
    rng_state: u64,
    /// Same parked-victim slot as the base filter: an eviction chain that
    /// runs out of moves holds a fingerprint already in the set, so dropping
    /// it would be a false negative.
    victim_fp: u16,
    victim_bucket: usize,
}

impl VariableFpCuckooFilter {
    pub fn new(expected_entries: usize, width: FingerprintWidth) -> Self {
        let needed = (expected_entries.max(1) * 105 / 100) / BUCKET_SIZE + 1;
        let num_buckets = needed.max(2).next_power_of_two();
        Self {
            width,
            buckets: vec![[0u16; BUCKET_SIZE]; num_buckets],
            mask: num_buckets - 1,
            count: 0,
            rng_state: 0x9e3779b97f4a7c15,
            victim_fp: 0,
            victim_bucket: 0,
        }
    }

    pub fn width(&self) -> FingerprintWidth {
        self.width
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

    /// Expected false-positive rate at the current occupancy for this width:
    /// `1 - (1 - 2^-f)^(2 * b * alpha)`.
    pub fn estimated_fpp(&self) -> f64 {
        let alpha = self.count as f64 / (self.buckets.len() * BUCKET_SIZE) as f64;
        if alpha <= 0.0 {
            return 0.0;
        }
        let per_slot = 1.0 - 2f64.powi(-(self.width.bits() as i32));
        1.0 - per_slot.powf(2.0 * BUCKET_SIZE as f64 * alpha)
    }

    pub fn clear(&mut self) {
        self.buckets.fill([0u16; BUCKET_SIZE]);
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
            std::mem::swap(&mut victim, &mut self.buckets[bucket_idx][slot]);
            bucket_idx ^= alt_index_of_fp_u16(victim) & self.mask;
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

    fn victim_matches(&self, fp: u16, i1: usize, i2: usize) -> bool {
        self.victim_fp == fp && (self.victim_bucket == i1 || self.victim_bucket == i2)
    }

    fn rehome_victim(&mut self) {
        if self.victim_fp == 0 {
            return;
        }
        let fp = self.victim_fp;
        let alt = (self.victim_bucket ^ alt_index_of_fp_u16(fp)) & self.mask;
        if self.try_place(self.victim_bucket, fp) || self.try_place(alt, fp) {
            self.victim_fp = 0;
        }
    }

    fn indices(&self, key: &str) -> (u16, usize, usize) {
        let h = mix(fnv1a64(key.as_bytes()));
        // Take the low `width` bits as the fingerprint. Force non-zero
        // so we can reuse 0 as the empty-slot sentinel.
        let raw = (h as u16) & self.width.mask();
        let fp = if raw == 0 { 1 } else { raw };
        let i1 = (h >> 16) as usize & self.mask;
        // alt() uses the same odd-constant trick as the base filter;
        // we widen to u16 input but the math is identical.
        let i2 = (i1 ^ alt_index_of_fp_u16(fp)) & self.mask;
        (fp, i1, i2)
    }

    fn try_place(&mut self, i: usize, fp: u16) -> bool {
        for slot in &mut self.buckets[i] {
            if *slot == 0 {
                *slot = fp;
                return true;
            }
        }
        false
    }

    fn bucket_has(&self, i: usize, fp: u16) -> bool {
        self.buckets[i].contains(&fp)
    }

    fn bucket_remove(&mut self, i: usize, fp: u16) -> bool {
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

fn alt_index_of_fp_u16(fp: u16) -> usize {
    // alt_index uses the byte-truncated fingerprint to stay
    // wire-compatible with the base; widening to u16 changes nothing
    // observable because the result is mod num_buckets.
    if fp <= u8::MAX as u16 {
        alt_index_of_fp(fp as u8)
    } else {
        (fp as u64).wrapping_mul(0x5bd1e9955_u64) as usize
    }
}

#[cfg(test)]
#[path = "variable_fingerprint_tests.rs"]
mod tests;
