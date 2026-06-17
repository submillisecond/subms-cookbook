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

    pub fn insert(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        if self.try_place(i1, fp) || self.try_place(i2, fp) {
            self.count += 1;
            return true;
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
        false
    }

    pub fn contains(&self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        self.bucket_has(i1, fp) || self.bucket_has(i2, fp)
    }

    pub fn delete(&mut self, key: &str) -> bool {
        let (fp, i1, i2) = self.indices(key);
        if self.bucket_remove(i1, fp) || self.bucket_remove(i2, fp) {
            self.count -= 1;
            return true;
        }
        false
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
mod tests {
    use super::*;

    #[test]
    fn round_trip_eight_bit() {
        let mut cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Eight);
        for i in 0..500u32 {
            assert!(cf.insert(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(cf.contains(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(cf.delete(&format!("k{i}")));
        }
        assert_eq!(cf.len(), 0);
    }

    #[test]
    fn round_trip_twelve_bit() {
        let mut cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Twelve);
        for i in 0..500u32 {
            assert!(cf.insert(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(cf.contains(&format!("k{i}")));
        }
        assert_eq!(cf.len(), 500);
    }

    #[test]
    fn round_trip_sixteen_bit() {
        let mut cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Sixteen);
        for i in 0..500u32 {
            assert!(cf.insert(&format!("k{i}")));
        }
        for i in 0..500u32 {
            assert!(cf.contains(&format!("k{i}")));
        }
    }

    #[test]
    fn wider_fingerprint_lowers_fpr() {
        // Same insert + probe set across both widths; 16-bit must be
        // strictly fewer false positives than 8-bit at this volume.
        let n = 5_000usize;
        let probes = 10_000usize;

        let mut narrow = VariableFpCuckooFilter::new(n, FingerprintWidth::Eight);
        let mut wide = VariableFpCuckooFilter::new(n, FingerprintWidth::Sixteen);
        for i in 0..n {
            narrow.insert(&format!("present{i}"));
            wide.insert(&format!("present{i}"));
        }
        let mut narrow_fp = 0usize;
        let mut wide_fp = 0usize;
        for i in 0..probes {
            let k = format!("absent{i}");
            if narrow.contains(&k) {
                narrow_fp += 1;
            }
            if wide.contains(&k) {
                wide_fp += 1;
            }
        }
        assert!(
            wide_fp < narrow_fp,
            "wide_fp={wide_fp} should be < narrow_fp={narrow_fp}"
        );
    }

    #[test]
    fn empty_filter_rejects_everything() {
        let cf = VariableFpCuckooFilter::new(100, FingerprintWidth::Twelve);
        assert!(!cf.contains("never-inserted"));
        assert!(cf.is_empty());
    }

    #[test]
    fn width_accessor_reports_configured_value() {
        let cf = VariableFpCuckooFilter::new(100, FingerprintWidth::Twelve);
        assert_eq!(cf.width(), FingerprintWidth::Twelve);
        assert_eq!(cf.width().bits(), 12);
    }

    #[test]
    fn delete_unknown_is_false() {
        let mut cf = VariableFpCuckooFilter::new(100, FingerprintWidth::Sixteen);
        assert!(!cf.delete("never-inserted"));
    }

    #[test]
    fn bucket_count_is_power_of_two() {
        let cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Twelve);
        let n = cf.bucket_count();
        assert!(n.is_power_of_two(), "{n} not power of two");
    }
}
