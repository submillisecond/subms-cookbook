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
mod tests {
    use super::*;

    fn build(records: &[&[u8]]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut w = Crc32SegmentWriter::new(&mut buf);
        for r in records {
            w.write(r).unwrap();
        }
        buf
    }

    #[test]
    fn round_trip_with_checksum() {
        let buf = build(&[b"alice", b"bob", b"carol"]);
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
        assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
        assert_eq!(r.next_record().unwrap().unwrap(), b"carol");
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn empty_segment_yields_none() {
        let mut r = Crc32SegmentReader::new(&[][..]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn corrupted_payload_detected() {
        let mut buf = build(&[b"hello"]);
        // Header (4) + payload (5). Flip one bit in the payload.
        buf[4] ^= 0x80;
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
    }

    #[test]
    fn corrupted_trailer_detected() {
        let mut buf = build(&[b"hello"]);
        // Trailer sits at the last 4 bytes; corrupt one.
        let last = buf.len() - 1;
        buf[last] ^= 0xff;
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::ChecksumMismatch)));
    }

    #[test]
    fn truncated_trailer_surfaces_typed_error() {
        let mut buf = build(&[b"hello"]);
        buf.truncate(buf.len() - 2); // chop 2 bytes of the trailer
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
    }

    #[test]
    fn truncated_payload_surfaces_typed_error() {
        let mut buf = Vec::new();
        buf.extend_from_slice(&[0, 0, 0, 10]); // claim 10 bytes
        buf.extend_from_slice(b"abc");
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
    }

    #[test]
    fn zero_length_record_round_trips() {
        let buf = build(&[&[], b"after-empty"]);
        let mut r = Crc32SegmentReader::new(buf.as_slice());
        assert_eq!(r.next_record().unwrap().unwrap(), b"");
        assert_eq!(r.next_record().unwrap().unwrap(), b"after-empty");
    }
}
