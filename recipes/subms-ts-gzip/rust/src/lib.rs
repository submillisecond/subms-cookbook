//! `subms-ts-gzip` - a zero-dep gzip codec that wraps any inner
//! [`TsCodec`](subms_ts::TsCodec). It hand-rolls DEFLATE/INFLATE (RFC 1951) and
//! the gzip container (RFC 1952) - no `flate2`, no `java.util.zip.Deflater`,
//! nothing on the wire but bytes this crate wrote itself.
//!
//! The output is a real gzip stream: the 10-byte header, a raw DEFLATE body,
//! and the CRC-32 + ISIZE trailer. That makes [`TsGzipCodec::encode`] output
//! `gunzip`-able, and [`TsGzipCodec::decode`] can read arbitrary gzip/zlib
//! output (it decodes stored, fixed-Huffman, AND dynamic-Huffman blocks).
//!
//! Compose it over any inner codec: `gzip+json`, `gzip+cbor`, etc. The wrapper
//! is value-type agnostic; the inner codec owns the `TsSeries<T>` <-> bytes
//! shape and the wrapper only compresses the result.
//!
//! ```
//! use subms_ts::{TsCodec, TsSeries};
//! use subms_ts::TsJsonCodec;
//! use subms_ts_gzip::TsGzipCodec;
//!
//! let mut s = TsSeries::<f64>::new();
//! s.push(1, 1.5).unwrap();
//! s.push(2, 2.5).unwrap();
//! let codec = TsGzipCodec::new(TsJsonCodec::new(), 6);
//! let bytes = codec.encode(&s);
//! let back = codec.decode(&bytes).unwrap();
//! assert_eq!(back.len(), 2);
//! assert_eq!(codec.format(), "gzip+json");
//! ```

use std::marker::PhantomData;

use subms_ts::{TsCodec, TsSeries};

mod crc32;
mod deflate;
mod inflate;

#[cfg(feature = "harness")]
pub mod recipe;

pub use inflate::InflateError;

/// The 10-byte gzip header: magic `1f 8b`, CM=8 (deflate), FLG=0, MTIME=0 (4),
/// XFL=0, OS=255 (unknown). MTIME stays zero so encode is deterministic.
const GZIP_HEADER: [u8; 10] = [0x1f, 0x8b, 0x08, 0x00, 0, 0, 0, 0, 0x00, 0xff];

/// gzip-framing / DEFLATE failure on decode. The inner codec's own error is
/// carried separately in [`TsGzipCodecError`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsGzipError {
    /// Header shorter than 10 bytes, or trailer missing.
    Truncated,
    /// First two bytes were not `1f 8b`.
    BadMagic,
    /// CM != 8 (only DEFLATE is defined).
    BadMethod(u8),
    /// A header flag we do not support was set (we never emit FLG bits, but a
    /// real gzip may; FEXTRA / FNAME / FCOMMENT are skipped, others rejected).
    UnsupportedFlag(u8),
    /// The DEFLATE body failed to inflate.
    Inflate(InflateError),
    /// Trailer CRC-32 did not match the inflated bytes.
    CrcMismatch { expected: u32, actual: u32 },
    /// Trailer ISIZE did not match the inflated length (mod 2^32).
    SizeMismatch { expected: u32, actual: u32 },
}

impl std::fmt::Display for TsGzipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsGzipError::Truncated => write!(f, "truncated gzip stream"),
            TsGzipError::BadMagic => write!(f, "bad gzip magic (expected 1f 8b)"),
            TsGzipError::BadMethod(m) => write!(f, "unsupported gzip method {m} (expected 8)"),
            TsGzipError::UnsupportedFlag(b) => write!(f, "unsupported gzip flag bits {b:#x}"),
            TsGzipError::Inflate(e) => write!(f, "inflate failed: {e}"),
            TsGzipError::CrcMismatch { expected, actual } => {
                write!(f, "gzip CRC mismatch: expected {expected:#x}, got {actual:#x}")
            }
            TsGzipError::SizeMismatch { expected, actual } => {
                write!(f, "gzip ISIZE mismatch: expected {expected}, got {actual}")
            }
        }
    }
}

impl std::error::Error for TsGzipError {}

/// Decode failure: either the gzip layer (`Gzip`) or the inner codec (`Inner`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsGzipCodecError<E> {
    Gzip(TsGzipError),
    Inner(E),
}

impl<E: std::fmt::Display> std::fmt::Display for TsGzipCodecError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsGzipCodecError::Gzip(e) => write!(f, "{e}"),
            TsGzipCodecError::Inner(e) => write!(f, "inner codec: {e}"),
        }
    }
}

impl<E: std::fmt::Debug + std::fmt::Display> std::error::Error for TsGzipCodecError<E> {}

/// Wrap raw bytes in a gzip container: header + DEFLATE(payload) + CRC32 + ISIZE.
pub fn gzip(payload: &[u8], level: u32) -> Vec<u8> {
    let body = deflate::deflate(payload, level);
    let mut out = Vec::with_capacity(body.len() + 18);
    out.extend_from_slice(&GZIP_HEADER);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32::crc32(payload).to_le_bytes());
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out
}

/// Unwrap a gzip container and inflate the body, verifying CRC32 + ISIZE.
/// Accepts any standards-compliant gzip stream (stored / fixed / dynamic).
pub fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, TsGzipError> {
    if bytes.len() < 18 {
        return Err(TsGzipError::Truncated);
    }
    if bytes[0] != 0x1f || bytes[1] != 0x8b {
        return Err(TsGzipError::BadMagic);
    }
    if bytes[2] != 8 {
        return Err(TsGzipError::BadMethod(bytes[2]));
    }
    let flg = bytes[3];
    // Bits: FTEXT(1) FHCRC(2) FEXTRA(4) FNAME(8) FCOMMENT(16). Anything above
    // 0x1f is reserved and unsupported.
    if flg & 0xe0 != 0 {
        return Err(TsGzipError::UnsupportedFlag(flg));
    }
    let mut pos = 10usize;
    if flg & 0x04 != 0 {
        // FEXTRA: 2-byte little-endian length then that many bytes.
        if pos + 2 > bytes.len() {
            return Err(TsGzipError::Truncated);
        }
        let xlen = u16::from_le_bytes([bytes[pos], bytes[pos + 1]]) as usize;
        pos += 2 + xlen;
    }
    if flg & 0x08 != 0 {
        pos = skip_cstr(bytes, pos)?;
    }
    if flg & 0x10 != 0 {
        pos = skip_cstr(bytes, pos)?;
    }
    if flg & 0x02 != 0 {
        // FHCRC: 2-byte header CRC16, skip it (we don't verify the header crc).
        pos += 2;
    }
    if pos + 8 > bytes.len() {
        return Err(TsGzipError::Truncated);
    }
    let body = &bytes[pos..bytes.len() - 8];
    let trailer = &bytes[bytes.len() - 8..];
    let out = inflate::inflate(body).map_err(TsGzipError::Inflate)?;

    let want_crc = u32::from_le_bytes([trailer[0], trailer[1], trailer[2], trailer[3]]);
    let got_crc = crc32::crc32(&out);
    if want_crc != got_crc {
        return Err(TsGzipError::CrcMismatch {
            expected: want_crc,
            actual: got_crc,
        });
    }
    let want_size = u32::from_le_bytes([trailer[4], trailer[5], trailer[6], trailer[7]]);
    let got_size = out.len() as u32;
    if want_size != got_size {
        return Err(TsGzipError::SizeMismatch {
            expected: want_size,
            actual: got_size,
        });
    }
    Ok(out)
}

fn skip_cstr(bytes: &[u8], mut pos: usize) -> Result<usize, TsGzipError> {
    while pos < bytes.len() {
        let b = bytes[pos];
        pos += 1;
        if b == 0 {
            return Ok(pos);
        }
    }
    Err(TsGzipError::Truncated)
}

/// A gzip codec wrapping an inner `TsCodec<T>`. `encode` gzips the inner
/// encoding; `decode` gunzips then delegates to the inner decoder.
#[derive(Clone, Debug)]
pub struct TsGzipCodec<C, T> {
    inner: C,
    level: u32,
    format: String,
    _t: PhantomData<T>,
}

impl<C: TsCodec<T>, T> TsGzipCodec<C, T> {
    /// `level` 0 = stored only, 1..=3 = greedy LZ77, 4..=9 = lazy matching with
    /// growing match-chain effort. Values above 9 clamp to 9.
    pub fn new(inner: C, level: u32) -> Self {
        let level = level.min(9);
        let format = format!("gzip+{}", inner.format());
        Self {
            inner,
            level,
            format,
            _t: PhantomData,
        }
    }

    pub fn level(&self) -> u32 {
        self.level
    }

    pub fn inner(&self) -> &C {
        &self.inner
    }
}

impl<C: TsCodec<T>, T> TsCodec<T> for TsGzipCodec<C, T> {
    type Error = TsGzipCodecError<C::Error>;

    fn encode(&self, series: &TsSeries<T>) -> Vec<u8> {
        gzip(&self.inner.encode(series), self.level)
    }

    fn decode(&self, bytes: &[u8]) -> Result<TsSeries<T>, Self::Error> {
        let payload = gunzip(bytes).map_err(TsGzipCodecError::Gzip)?;
        self.inner.decode(&payload).map_err(TsGzipCodecError::Inner)
    }

    fn format(&self) -> &str {
        &self.format
    }
}
