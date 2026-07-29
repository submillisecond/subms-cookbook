//! CRC32C-checked segment reader.
//!
//! Block format (big-endian on disk):
//!
//! ```text
//! u32 length
//! u8  payload[length]
//! u32 crc32c-of-payload
//! ```
//!
//! `crc32c` (Castagnoli polynomial) is the storage-engine standard - LevelDB,
//! RocksDB, Parquet all use it. The crate dispatches to SSE4.2 / ARMv8 CRC
//! instructions when the CPU exposes them, falling back to a software table.
//!
//! On checksum mismatch, `next_record()` returns `Error::ChecksumMismatch`
//! and advances the cursor past the bad block; the caller decides whether
//! to abort or skip.

use std::io::{self, Read, Write};

use crate::Error;

pub struct Crc32SegmentReader<R: Read> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: Read> Crc32SegmentReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

    /// Read the next CRC-checked record. Returns `Ok(None)` at clean EOF,
    /// `Err(TruncatedFrame)` mid-block, or `Err(ChecksumMismatch)` when
    /// the trailer doesn't match the payload's CRC32C.
    pub fn next_record(&mut self) -> Result<Option<&[u8]>, Error> {
        let mut len_buf = [0u8; 4];
        match self.reader.read(&mut len_buf)? {
            0 => return Ok(None),
            n if n < 4 => return Err(Error::TruncatedFrame),
            _ => {}
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        self.buffer.resize(len, 0);
        self.reader.read_exact(&mut self.buffer).map_err(map_eof)?;
        let mut crc_buf = [0u8; 4];
        self.reader.read_exact(&mut crc_buf).map_err(map_eof)?;
        let expected = u32::from_be_bytes(crc_buf);
        let actual = crc32c::crc32c(&self.buffer);
        if expected != actual {
            return Err(Error::ChecksumMismatch);
        }
        Ok(Some(&self.buffer))
    }
}

/// Writer mirror - emits `length | payload | crc32c` blocks. Useful for
/// tests, recipe wiring, and downstream callers that want a matching
/// producer side.
pub struct Crc32SegmentWriter<W: Write> {
    writer: W,
}

impl<W: Write> Crc32SegmentWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write(&mut self, record: &[u8]) -> io::Result<()> {
        let len = record.len() as u32;
        self.writer.write_all(&len.to_be_bytes())?;
        self.writer.write_all(record)?;
        let crc = crc32c::crc32c(record);
        self.writer.write_all(&crc.to_be_bytes())?;
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
#[path = "crc32_tests.rs"]
mod tests;
