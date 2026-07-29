//! xxHash3 (64-bit) checksum reader. Block format:
//!
//! ```text
//! u32 length
//! u8  payload[length]
//! u64 xxh3-64-of-payload
//! ```
//!
//! Faster than CRC32C on modern CPUs - xxh3 burns about 0.3 ns/byte on the
//! sub-ms reference rig; CRC32C is closer to 0.7 ns/byte once you're past
//! the CRC instruction's pipeline floor. Not designed for adversarial
//! inputs (no collision-resistance guarantee); pick `crc32` instead when
//! the segment lives somewhere an attacker can touch.

use std::io::{self, Read, Write};

use xxhash_rust::xxh3::xxh3_64;

use crate::Error;

pub struct Xxh3SegmentReader<R: Read> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: Read> Xxh3SegmentReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

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
        let mut hash_buf = [0u8; 8];
        self.reader.read_exact(&mut hash_buf).map_err(map_eof)?;
        let expected = u64::from_be_bytes(hash_buf);
        let actual = xxh3_64(&self.buffer);
        if expected != actual {
            return Err(Error::ChecksumMismatch);
        }
        Ok(Some(&self.buffer))
    }
}

pub struct Xxh3SegmentWriter<W: Write> {
    writer: W,
}

impl<W: Write> Xxh3SegmentWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write(&mut self, record: &[u8]) -> io::Result<()> {
        let len = record.len() as u32;
        self.writer.write_all(&len.to_be_bytes())?;
        self.writer.write_all(record)?;
        let hash = xxh3_64(record);
        self.writer.write_all(&hash.to_be_bytes())?;
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
#[path = "xxh3_tests.rs"]
mod tests;
