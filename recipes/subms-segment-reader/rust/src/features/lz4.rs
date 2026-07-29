//! LZ4 block decompression.
//!
//! Block format (big-endian on disk):
//!
//! ```text
//! u8  algo_tag       (0 = stored, 1 = lz4)
//! u32 uncompressed_len
//! u32 compressed_len
//! u8  payload[compressed_len]
//! ```
//!
//! A 0 tag means the payload is stored verbatim (writer chose not to
//! compress, typically because the block was incompressible). A 1 tag
//! means `lz4_flex::decompress_size_prepended` is wrong - we hold the
//! uncompressed length out-of-band in the header, so we call the
//! size-known variant directly. Any other tag is rejected.

use std::io::{self, Read, Write};

use crate::Error;

pub const TAG_STORED: u8 = 0;
pub const TAG_LZ4: u8 = 1;

pub struct Lz4SegmentReader<R: Read> {
    reader: R,
    /// Holds the most recently decompressed payload so we can hand
    /// back a borrowed slice.
    out: Vec<u8>,
    /// Scratch buffer for the on-disk compressed payload.
    scratch: Vec<u8>,
}

impl<R: Read> Lz4SegmentReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            out: Vec::new(),
            scratch: Vec::new(),
        }
    }

    pub fn next_record(&mut self) -> Result<Option<&[u8]>, Error> {
        let mut tag_buf = [0u8; 1];
        if self.reader.read(&mut tag_buf)? == 0 {
            return Ok(None);
        }
        let tag = tag_buf[0];

        let mut hdr = [0u8; 8];
        self.reader.read_exact(&mut hdr).map_err(map_eof)?;
        let uncompressed_len = u32::from_be_bytes(hdr[0..4].try_into().unwrap()) as usize;
        let compressed_len = u32::from_be_bytes(hdr[4..8].try_into().unwrap()) as usize;

        self.scratch.resize(compressed_len, 0);
        self.reader.read_exact(&mut self.scratch).map_err(map_eof)?;

        match tag {
            TAG_STORED => {
                if compressed_len != uncompressed_len {
                    return Err(Error::DecompressionFailed);
                }
                self.out.clear();
                self.out.extend_from_slice(&self.scratch);
            }
            TAG_LZ4 => {
                self.out = lz4_flex::decompress(&self.scratch, uncompressed_len)
                    .map_err(|_| Error::DecompressionFailed)?;
            }
            _ => return Err(Error::DecompressionFailed),
        }
        Ok(Some(&self.out))
    }
}

/// Block writer matched to `Lz4SegmentReader`. Picks `stored` when the
/// LZ4 output would be larger than the raw payload (small / random
/// blocks), and `lz4` otherwise.
pub struct Lz4BlockWriter<W: Write> {
    writer: W,
}

impl<W: Write> Lz4BlockWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    /// Write a block, picking the smaller of stored / lz4 encodings.
    pub fn write(&mut self, payload: &[u8]) -> io::Result<()> {
        let compressed = lz4_flex::compress(payload);
        if compressed.len() < payload.len() {
            self.write_block(TAG_LZ4, payload.len() as u32, &compressed)
        } else {
            self.write_block(TAG_STORED, payload.len() as u32, payload)
        }
    }

    /// Force-write an LZ4 block even if larger than the stored form.
    /// Useful for tests that want to assert LZ4 path coverage.
    pub fn write_lz4(&mut self, payload: &[u8]) -> io::Result<()> {
        let compressed = lz4_flex::compress(payload);
        self.write_block(TAG_LZ4, payload.len() as u32, &compressed)
    }

    /// Force-write a stored block.
    pub fn write_stored(&mut self, payload: &[u8]) -> io::Result<()> {
        self.write_block(TAG_STORED, payload.len() as u32, payload)
    }

    fn write_block(&mut self, tag: u8, uncompressed_len: u32, body: &[u8]) -> io::Result<()> {
        self.writer.write_all(&[tag])?;
        self.writer.write_all(&uncompressed_len.to_be_bytes())?;
        self.writer.write_all(&(body.len() as u32).to_be_bytes())?;
        self.writer.write_all(body)?;
        Ok(())
    }
}

fn map_eof(e: io::Error) -> Error {
    if e.kind() == io::ErrorKind::UnexpectedEof {
        Error::TruncatedFrame
    } else {
        Error::Io(e)
    }
}

#[cfg(test)]
#[path = "lz4_tests.rs"]
mod tests;
