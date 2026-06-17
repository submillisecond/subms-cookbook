//! LZ4 block compression for SSTable data blocks.
//!
//! Block format (big-endian):
//! ```text
//! marker:    u8   = 0x4C ('L')
//! algo:      u8   = 0x01 (lz4 frame)
//! uncomp:    u32  uncompressed byte length
//! data:      bytes
//! ```
//!
//! Built on `lz4_flex` (pure-Rust LZ4 block codec). The wrapper falls back
//! to a stored-as-is encoding when the compressed bytes are not smaller
//! than the input (small inputs / incompressible payloads); the algo byte
//! discriminates between `lz4` (0x01) and `stored` (0x00). Decompression
//! handles both.

use std::io;

pub struct Lz4BlockCompressor;

const MARKER: u8 = 0x4C;
const ALGO_STORED: u8 = 0x00;
const ALGO_LZ4: u8 = 0x01;
const HEADER_LEN: usize = 1 + 1 + 4;

impl Lz4BlockCompressor {
    pub fn new() -> Self {
        Self
    }

    /// Compress `block` into a self-describing buffer. The output starts
    /// with a 6-byte header so [`Self::decompress`] knows the uncompressed
    /// size and which path to take.
    pub fn compress(&self, block: &[u8]) -> Vec<u8> {
        let compressed = lz4_flex::compress(block);
        let mut out = Vec::with_capacity(HEADER_LEN + compressed.len().max(block.len()));
        out.push(MARKER);
        // If lz4 didn't shrink, store raw to keep the decode path branch-free
        // and avoid pathological inflation on already-compressed blocks.
        if compressed.len() < block.len() {
            out.push(ALGO_LZ4);
            out.extend_from_slice(&(block.len() as u32).to_be_bytes());
            out.extend_from_slice(&compressed);
        } else {
            out.push(ALGO_STORED);
            out.extend_from_slice(&(block.len() as u32).to_be_bytes());
            out.extend_from_slice(block);
        }
        out
    }

    /// Decode a buffer written by [`Self::compress`]. Errors on bad marker,
    /// unknown algo byte, length mismatch, or LZ4 corruption.
    pub fn decompress(&self, buf: &[u8]) -> io::Result<Vec<u8>> {
        if buf.len() < HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "lz4 block: too short",
            ));
        }
        if buf[0] != MARKER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "lz4 block: bad marker",
            ));
        }
        let algo = buf[1];
        let uncomp_len = u32::from_be_bytes(buf[2..6].try_into().unwrap()) as usize;
        let payload = &buf[HEADER_LEN..];
        match algo {
            ALGO_STORED => {
                if payload.len() != uncomp_len {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "lz4 block: stored payload size mismatch",
                    ));
                }
                Ok(payload.to_vec())
            }
            ALGO_LZ4 => lz4_flex::decompress(payload, uncomp_len).map_err(|e| {
                io::Error::new(io::ErrorKind::InvalidData, format!("lz4 decode: {e}"))
            }),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("lz4 block: unknown algo byte 0x{other:02x}"),
            )),
        }
    }
}

impl Default for Lz4BlockCompressor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repetitive(n: usize) -> Vec<u8> {
        let pattern = b"the-quick-brown-fox-jumps-over-the-lazy-dog-";
        let mut out = Vec::with_capacity(n);
        while out.len() < n {
            out.extend_from_slice(pattern);
        }
        out.truncate(n);
        out
    }

    #[test]
    fn round_trip_compressible() {
        let c = Lz4BlockCompressor::new();
        let input = repetitive(4096);
        let enc = c.compress(&input);
        assert!(
            enc.len() < input.len(),
            "compressible payload should shrink"
        );
        let dec = c.decompress(&enc).unwrap();
        assert_eq!(dec, input);
    }

    #[test]
    fn round_trip_empty() {
        let c = Lz4BlockCompressor::new();
        let enc = c.compress(&[]);
        let dec = c.decompress(&enc).unwrap();
        assert!(dec.is_empty());
    }

    #[test]
    fn incompressible_falls_back_to_stored() {
        let c = Lz4BlockCompressor::new();
        // Pseudorandom-but-deterministic bytes (xorshift) - LZ4 won't shrink these.
        let mut s = 0x9E37_79B9_u32;
        let mut input = Vec::with_capacity(2048);
        for _ in 0..2048 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            input.push(s as u8);
        }
        let enc = c.compress(&input);
        // Algo byte should indicate stored path - second byte of the header.
        assert_eq!(enc[1], ALGO_STORED, "incompressible falls back to stored");
        let dec = c.decompress(&enc).unwrap();
        assert_eq!(dec, input);
    }

    #[test]
    fn bad_marker_errors() {
        let c = Lz4BlockCompressor::new();
        let mut enc = c.compress(b"hello world hello world");
        enc[0] = 0xff;
        let err = c.decompress(&enc).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn too_short_buffer_errors() {
        let c = Lz4BlockCompressor::new();
        let err = c.decompress(&[0u8; 3]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn unknown_algo_byte_errors() {
        let c = Lz4BlockCompressor::new();
        let mut enc = c.compress(b"abcdef");
        enc[1] = 0x7f;
        assert!(c.decompress(&enc).is_err());
    }
}
