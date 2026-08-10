//! Typed error surface.
//!
//! Merge and the wire codec both fail for reasons a caller can act on - a
//! precision mismatch is a config bug, a truncated buffer is a transport bug -
//! so the failure names itself instead of returning a string.

use core::fmt;

/// Every failure `subms-hyperloglog` can return.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum HllError {
    /// Two sketches at different precisions cannot be reconciled: the register
    /// index is cut from a different slice of the hash.
    PrecisionMismatch { left: u32, right: u32 },
    /// Precision outside the calibrated `[4, 18]` range.
    InvalidPrecision(u32),
    /// Buffer does not start with the `SHLL` magic.
    BadMagic,
    /// Format version this build does not understand.
    UnsupportedVersion(u8),
    /// Encoding byte this build does not understand, or one the target type
    /// refuses (a sparse buffer handed to `HyperLogLog::from_bytes`).
    UnsupportedEncoding(u8),
    /// Buffer ended before the declared payload did.
    Truncated { expected: usize, actual: usize },
}

impl fmt::Display for HllError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HllError::PrecisionMismatch { left, right } => {
                write!(f, "precision mismatch: {left} vs {right}")
            }
            HllError::InvalidPrecision(p) => write!(f, "precision {p} outside [4, 18]"),
            HllError::BadMagic => write!(f, "bad magic: not a subms-hyperloglog buffer"),
            HllError::UnsupportedVersion(v) => write!(f, "unsupported format version {v}"),
            HllError::UnsupportedEncoding(e) => write!(f, "unsupported encoding {e}"),
            HllError::Truncated { expected, actual } => {
                write!(
                    f,
                    "truncated buffer: expected {expected} bytes, got {actual}"
                )
            }
        }
    }
}

impl std::error::Error for HllError {}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
