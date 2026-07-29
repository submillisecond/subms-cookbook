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
//! [`ZstdBlockCompressor::with_level`] to override. Levels outside 1..=22 are clamped to
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
#[path = "zstd_tests.rs"]
mod tests;
