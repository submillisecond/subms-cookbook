//! Minimal bloom filter - standalone, reusable, zero-dependency.
//!
//! Standard double-hashed bloom filter: FNV-1a 64-bit produces two 32-bit
//! subhashes for the double-hashing trick. Sizing defaults to ~10 bits per
//! key and k=7, which gives ~1% false-positive rate. Suitable as a building
//! block for other cookbook samples (LSM tree SSTables, in particular).
//!
//! The on-disk layout is fixed and language-agnostic:
//!
//! ```text
//! bit_count: u32 (big-endian)
//! k:         u32 (big-endian)
//! words:     u32 (big-endian) - number of u64 words
//! bits:      (u64 big-endian) * words
//! ```

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature modules. Each is independent of the base filter and
// gated by its own Cargo feature; `cargo add subms-bloom-filter` alone
// keeps the base zero-dep + std-only shape.
//
// See README and the cookbook page for the per-feature p99 numbers,
// memory cost, and composition guidance.
#[cfg(any(feature = "counting", feature = "scalable", feature = "partitioned"))]
pub mod features;

#[cfg(feature = "counting")]
pub use features::counting::CountingBloomFilter;
#[cfg(feature = "partitioned")]
pub use features::partitioned::PartitionedBloomFilter;
#[cfg(feature = "scalable")]
pub use features::scalable::ScalableBloomFilter;

use std::io::{self, Write};

pub(crate) const FNV_OFFSET: u64 = 0xcbf29ce484222325;
pub(crate) const FNV_PRIME: u64 = 0x100000001b3;

pub struct BloomFilter {
    bit_count: u32,
    k: u32,
    bits: Vec<u64>,
}

impl BloomFilter {
    /// Build an empty filter sized for `expected_entries` at ~1% FPR
    /// (10 bits/key, k=7). The 64-bit floor matters when expected_entries is small.
    pub fn new(expected_entries: usize) -> Self {
        let bit_count = expected_entries.saturating_mul(10).max(64) as u32;
        let words = (bit_count as usize).div_ceil(64);
        Self {
            bit_count,
            k: 7,
            bits: vec![0u64; words],
        }
    }

    pub fn add(&mut self, key: &str) {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            self.bits[(idx / 64) as usize] |= 1u64 << (idx % 64);
        }
    }

    pub fn might_contain(&self, key: &str) -> bool {
        let h = fnv1a64(key);
        let h1 = h as u32;
        let h2 = ((h >> 32) as u32) | 1;
        for i in 0..self.k {
            let idx = h1.wrapping_add(i.wrapping_mul(h2)) % self.bit_count;
            if self.bits[(idx / 64) as usize] & (1u64 << (idx % 64)) == 0 {
                return false;
            }
        }
        true
    }

    pub fn bit_count(&self) -> u32 {
        self.bit_count
    }

    pub fn k(&self) -> u32 {
        self.k
    }

    pub fn write_to<W: Write>(&self, out: &mut W) -> io::Result<()> {
        out.write_all(&self.bit_count.to_be_bytes())?;
        out.write_all(&self.k.to_be_bytes())?;
        out.write_all(&(self.bits.len() as u32).to_be_bytes())?;
        for w in &self.bits {
            out.write_all(&w.to_be_bytes())?;
        }
        Ok(())
    }

    /// Parse a serialised bloom filter from `buf`. Errors if the buffer is
    /// shorter than the header or truncated mid-bits.
    pub fn parse(buf: &[u8]) -> io::Result<Self> {
        if buf.len() < 12 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bloom section too short",
            ));
        }
        let bit_count = u32::from_be_bytes(buf[0..4].try_into().unwrap());
        let k = u32::from_be_bytes(buf[4..8].try_into().unwrap());
        let words = u32::from_be_bytes(buf[8..12].try_into().unwrap()) as usize;
        if buf.len() < 12 + words * 8 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "bloom section truncated",
            ));
        }
        let mut bits = Vec::with_capacity(words);
        for i in 0..words {
            let off = 12 + i * 8;
            bits.push(u64::from_be_bytes(buf[off..off + 8].try_into().unwrap()));
        }
        Ok(Self { bit_count, k, bits })
    }
}

pub(crate) fn fnv1a64(key: &str) -> u64 {
    let mut h = FNV_OFFSET;
    for &b in key.as_bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(FNV_PRIME);
    }
    h
}
