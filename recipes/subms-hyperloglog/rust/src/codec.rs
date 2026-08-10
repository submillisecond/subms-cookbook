//! Canonical wire format. A sketch is worth ~1% of the bytes the raw ids
//! would cost, which only pays off if it can leave the process: checkpoint to
//! disk, ship a per-shard partial to a collector, cache a per-key sketch in
//! Redis. That needs a format both ports agree on byte for byte, and both
//! ports emit exactly the bytes below.
//!
//! ```text
//! 0..4   magic  "SHLL"
//! 4      format version (1)
//! 5      encoding: 0 dense, 1 sparse
//! 6      precision p
//! 7      reserved, zero
//! dense  8..8+m           m register bytes, one per register
//! sparse 8..12            u32 BE promotion threshold
//!        12..16           u32 BE entry count n
//!        16..16+5n        n * (u32 BE register index, u8 rho)
//! ```
//!
//! Multi-byte fields are big-endian, so a hex dump reads left to right and a
//! Java `DataOutputStream` needs no byte-order argument.
//!
//! This is the recipe's own format. It is not Redis's `PFADD` string and it is
//! not a DataSketches `HllSketch` image; neither will read these bytes.

use crate::{HllError, HyperLogLog, MAX_PRECISION, MIN_PRECISION};

/// Leading bytes of every buffer this codec writes.
pub const MAGIC: [u8; 4] = *b"SHLL";
/// Format version. Bumped only on a breaking layout change.
pub const FORMAT_VERSION: u8 = 1;

pub(crate) const ENC_DENSE: u8 = 0;
#[cfg(feature = "sparse")]
pub(crate) const ENC_SPARSE: u8 = 1;
pub(crate) const HEADER_LEN: usize = 8;

pub(crate) fn write_header(out: &mut Vec<u8>, encoding: u8, p: u32) {
    out.extend_from_slice(&MAGIC);
    out.push(FORMAT_VERSION);
    out.push(encoding);
    out.push(p as u8);
    out.push(0);
}

/// Validates magic, version and precision, returning `(encoding, p)`.
pub(crate) fn read_header(bytes: &[u8]) -> Result<(u8, u32), HllError> {
    if bytes.len() < HEADER_LEN {
        return Err(HllError::Truncated {
            expected: HEADER_LEN,
            actual: bytes.len(),
        });
    }
    if bytes[..4] != MAGIC {
        return Err(HllError::BadMagic);
    }
    if bytes[4] != FORMAT_VERSION {
        return Err(HllError::UnsupportedVersion(bytes[4]));
    }
    let p = u32::from(bytes[6]);
    if !(MIN_PRECISION..=MAX_PRECISION).contains(&p) {
        return Err(HllError::InvalidPrecision(p));
    }
    Ok((bytes[5], p))
}

#[cfg(feature = "sparse")]
pub(crate) fn read_u32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
}

impl HyperLogLog {
    /// Serialise to the canonical dense form: an 8-byte header then the raw
    /// register array. Length is always `8 + 2^p`, so a reader can size the
    /// allocation from the header alone.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(HEADER_LEN + self.registers.len());
        write_header(&mut out, ENC_DENSE, self.precision());
        out.extend_from_slice(&self.registers);
        out
    }

    /// Parse a dense buffer. A sparse buffer is rejected with
    /// `UnsupportedEncoding` rather than silently densified - use
    /// `SparseHyperLogLog::from_bytes`, which reads both.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, HllError> {
        let (encoding, p) = read_header(bytes)?;
        if encoding != ENC_DENSE {
            return Err(HllError::UnsupportedEncoding(encoding));
        }
        let m = 1usize << p;
        let expected = HEADER_LEN + m;
        if bytes.len() < expected {
            return Err(HllError::Truncated {
                expected,
                actual: bytes.len(),
            });
        }
        let mut hll = HyperLogLog::new(p);
        hll.registers
            .copy_from_slice(&bytes[HEADER_LEN..HEADER_LEN + m]);
        Ok(hll)
    }
}

#[cfg(test)]
#[path = "codec_tests.rs"]
mod tests;
