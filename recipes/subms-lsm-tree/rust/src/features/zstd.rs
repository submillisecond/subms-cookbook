//! Zstd block compression for SSTable data blocks.
//!
//! Block format mirrors the LZ4 wrapper exactly so the read path can dispatch
//! by the algo byte if multiple compressors share a file:
//! ```text
//! marker:    u8   = 0x5A ('Z')
//! algo:      u8   = 0x00 (stored) | 0x01 (zstd)
//! uncomp:    u32  uncompressed byte length
//! data:      bytes
//! ```
//!
//! Compression level defaults to 3 (zstd's default speed/ratio knee). Use
//! [`Self::with_level`] to override. Levels outside 1..=22 are clamped to
//! that range.

use std::io;

const MARKER: u8 = 0x5A;
const ALGO_STORED: u8 = 0x00;
const ALGO_ZSTD: u8 = 0x01;
const HEADER_LEN: usize = 1 + 1 + 4;
const DEFAULT_LEVEL: i32 = 3;
const MIN_LEVEL: i32 = 1;
const MAX_LEVEL: i32 = 22;

pub struct ZstdBlockCompressor {
    level: i32,
}

impl ZstdBlockCompressor {
    pub fn new() -> Self {
        Self {
            level: DEFAULT_LEVEL,
        }
    }

    pub fn with_level(level: i32) -> Self {
        Self {
            level: level.clamp(MIN_LEVEL, MAX_LEVEL),
        }
    }

    pub fn level(&self) -> i32 {
        self.level
    }

    pub fn compress(&self, block: &[u8]) -> io::Result<Vec<u8>> {
        let compressed = zstd::bulk::compress(block, self.level)?;
        let mut out = Vec::with_capacity(HEADER_LEN + compressed.len().max(block.len()));
        out.push(MARKER);
        if compressed.len() < block.len() {
            out.push(ALGO_ZSTD);
            out.extend_from_slice(&(block.len() as u32).to_be_bytes());
            out.extend_from_slice(&compressed);
        } else {
            out.push(ALGO_STORED);
            out.extend_from_slice(&(block.len() as u32).to_be_bytes());
            out.extend_from_slice(block);
        }
        Ok(out)
    }

    pub fn decompress(&self, buf: &[u8]) -> io::Result<Vec<u8>> {
        if buf.len() < HEADER_LEN {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "zstd block: too short",
            ));
        }
        if buf[0] != MARKER {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "zstd block: bad marker",
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
                        "zstd block: stored payload size mismatch",
                    ));
                }
                Ok(payload.to_vec())
            }
            ALGO_ZSTD => zstd::bulk::decompress(payload, uncomp_len),
            other => Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("zstd block: unknown algo byte 0x{other:02x}"),
            )),
        }
    }
}

impl Default for ZstdBlockCompressor {
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
        let c = ZstdBlockCompressor::new();
        let input = repetitive(4096);
        let enc = c.compress(&input).unwrap();
        assert!(
            enc.len() < input.len(),
            "compressible payload should shrink"
        );
        let dec = c.decompress(&enc).unwrap();
        assert_eq!(dec, input);
    }

    #[test]
    fn round_trip_empty() {
        let c = ZstdBlockCompressor::new();
        let enc = c.compress(&[]).unwrap();
        let dec = c.decompress(&enc).unwrap();
        assert!(dec.is_empty());
    }

    #[test]
    fn level_is_clamped_to_valid_range() {
        let c_low = ZstdBlockCompressor::with_level(-99);
        assert_eq!(c_low.level(), MIN_LEVEL);
        let c_high = ZstdBlockCompressor::with_level(99);
        assert_eq!(c_high.level(), MAX_LEVEL);
        let c_mid = ZstdBlockCompressor::with_level(10);
        assert_eq!(c_mid.level(), 10);
    }

    #[test]
    fn incompressible_falls_back_to_stored() {
        let c = ZstdBlockCompressor::new();
        let mut s = 0xDEAD_BEEF_u32;
        let mut input = Vec::with_capacity(64);
        for _ in 0..64 {
            s ^= s << 13;
            s ^= s >> 17;
            s ^= s << 5;
            input.push(s as u8);
        }
        let enc = c.compress(&input).unwrap();
        // Tiny + random: zstd cannot shrink. Verify algo byte path.
        assert_eq!(enc[1], ALGO_STORED, "incompressible takes the stored path");
        let dec = c.decompress(&enc).unwrap();
        assert_eq!(dec, input);
    }

    #[test]
    fn bad_marker_errors() {
        let c = ZstdBlockCompressor::new();
        let mut enc = c.compress(b"hello world hello world hello world").unwrap();
        enc[0] = 0x00;
        let err = c.decompress(&enc).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn too_short_buffer_errors() {
        let c = ZstdBlockCompressor::new();
        let err = c.decompress(&[0u8; 3]).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidData);
    }

    #[test]
    fn unknown_algo_byte_errors() {
        let c = ZstdBlockCompressor::new();
        let mut enc = c.compress(b"abcdefghij").unwrap();
        enc[1] = 0x77;
        assert!(c.decompress(&enc).is_err());
    }
}
