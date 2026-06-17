//! Versioned wire format for a t-digest, byte-equivalent across Rust + Java:
//! `[version u8][compression f64][min f64][max f64][count u32][(mean f64,
//! weight f64) * count]`, all little-endian. A sketch serialized in one
//! language deserializes byte-for-byte in the other - encode on a shard,
//! merge on a coordinator.

use crate::{Centroid, TsTDigest};

const VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsTDigestError {
    BadVersion(u8),
    Truncated,
}

impl std::fmt::Display for TsTDigestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsTDigestError::BadVersion(v) => write!(f, "unknown t-digest version {v}"),
            TsTDigestError::Truncated => write!(f, "truncated t-digest bytes"),
        }
    }
}

impl std::error::Error for TsTDigestError {}

impl TsTDigest {
    /// Serialize the folded centroids. Folds any buffered points first (on a
    /// scratch clone, so `&self` is unchanged).
    pub fn serialize(&self) -> Vec<u8> {
        let mut d = self.clone();
        d.compact();
        let (compression, min, max, centroids) = d.parts();
        let mut out = Vec::with_capacity(1 + 28 + centroids.len() * 16);
        out.push(VERSION);
        out.extend_from_slice(&compression.to_le_bytes());
        out.extend_from_slice(&min.to_le_bytes());
        out.extend_from_slice(&max.to_le_bytes());
        out.extend_from_slice(&(centroids.len() as u32).to_le_bytes());
        for c in centroids {
            out.extend_from_slice(&c.mean.to_le_bytes());
            out.extend_from_slice(&c.weight.to_le_bytes());
        }
        out
    }

    pub fn deserialize(bytes: &[u8]) -> Result<Self, TsTDigestError> {
        if bytes.is_empty() {
            return Err(TsTDigestError::Truncated);
        }
        if bytes[0] != VERSION {
            return Err(TsTDigestError::BadVersion(bytes[0]));
        }
        if bytes.len() < 1 + 28 {
            return Err(TsTDigestError::Truncated);
        }
        let mut p = 1;
        let f64_at = |p: &mut usize| -> f64 {
            let v = f64::from_le_bytes(bytes[*p..*p + 8].try_into().unwrap());
            *p += 8;
            v
        };
        let compression = f64_at(&mut p);
        let min = f64_at(&mut p);
        let max = f64_at(&mut p);
        let count = u32::from_le_bytes(bytes[p..p + 4].try_into().unwrap()) as usize;
        p += 4;
        if bytes.len() < p + count * 16 {
            return Err(TsTDigestError::Truncated);
        }
        let mut centroids = Vec::with_capacity(count);
        for _ in 0..count {
            let mean = f64_at(&mut p);
            let weight = f64_at(&mut p);
            centroids.push(Centroid { mean, weight });
        }
        Ok(TsTDigest::from_parts(compression, min, max, centroids))
    }
}
