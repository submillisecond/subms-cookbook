//! Read length-prefix framed records from a segment file.
//!
//! ```
//! use subms_segment_reader::{SegmentWriter, SegmentReader};
//!
//! let mut buf = Vec::new();
//! { let mut w = SegmentWriter::new(&mut buf); w.write(b"alice").unwrap(); w.write(b"bob").unwrap(); }
//!
//! let mut r = SegmentReader::new(buf.as_slice());
//! assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
//! assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
//! assert!(r.next_record().unwrap().is_none());   // clean EOF
//! ```
//!
//! Frame format (big-endian on disk; matches the LSM SSTable and Java side):
//!
//! ```text
//! u32 length
//! u8  payload[length]
//! ```
//!
//! The reader streams record-by-record. Truncated tails surface as a typed
//! `Error::TruncatedFrame` instead of crashing - the recipe sees crash-recovery
//! workloads as common and the API treats them as expected.

use std::io::{self, Read, Write};

#[derive(Debug)]
pub enum Error {
    /// Underlying IO error.
    Io(io::Error),
    /// Header or payload truncated at the tail of the segment.
    TruncatedFrame,
    /// Block trailer's checksum did not match the payload's checksum.
    /// Raised by the `crc32` and `xxh3` opt-in readers.
    ChecksumMismatch,
    /// Block's algorithm tag is unknown or a decompress call failed.
    /// Raised by the `lz4` opt-in reader.
    DecompressionFailed,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self {
        Error::Io(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::TruncatedFrame => write!(f, "truncated frame at segment tail"),
            Error::ChecksumMismatch => write!(f, "block checksum mismatch"),
            Error::DecompressionFailed => write!(f, "block decompression failed"),
        }
    }
}

impl std::error::Error for Error {}

pub struct SegmentReader<R: Read> {
    reader: R,
    buffer: Vec<u8>,
}

impl<R: Read> SegmentReader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            buffer: Vec::new(),
        }
    }

    /// Read the next record. Returns `Ok(None)` at clean EOF;
    /// `Err(TruncatedFrame)` if the segment is cut in the middle of a frame.
    pub fn next_record(&mut self) -> Result<Option<&[u8]>, Error> {
        let mut len_buf = [0u8; 4];
        match self.reader.read(&mut len_buf)? {
            0 => return Ok(None),
            n if n < 4 => return Err(Error::TruncatedFrame),
            _ => {}
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        self.buffer.resize(len, 0);
        self.reader.read_exact(&mut self.buffer).map_err(|e| {
            if e.kind() == io::ErrorKind::UnexpectedEof {
                Error::TruncatedFrame
            } else {
                Error::Io(e)
            }
        })?;
        Ok(Some(&self.buffer))
    }
}

pub struct SegmentWriter<W: Write> {
    writer: W,
}

impl<W: Write> SegmentWriter<W> {
    pub fn new(writer: W) -> Self {
        Self { writer }
    }

    pub fn write(&mut self, record: &[u8]) -> io::Result<()> {
        let len = record.len() as u32;
        self.writer.write_all(&len.to_be_bytes())?;
        self.writer.write_all(record)?;
        Ok(())
    }

    pub fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

#[cfg(feature = "harness")]
pub mod recipe;

// Opt-in feature catalog. Each submodule is gated by its own Cargo
// feature flag and adds a capability without bloating the core build.
// See `Cargo.toml` `[features]` for the catalog.
#[cfg(any(
    feature = "mmap",
    feature = "crc32",
    feature = "xxh3",
    feature = "lz4",
    feature = "seek-index",
    feature = "wal-cursor",
))]
pub mod features;

#[cfg(feature = "crc32")]
pub use features::crc32::Crc32SegmentReader;
#[cfg(feature = "lz4")]
pub use features::lz4::{Lz4BlockWriter, Lz4SegmentReader};
#[cfg(feature = "mmap")]
pub use features::mmap::MmapSegmentReader;
#[cfg(feature = "seek-index")]
pub use features::seek_index::IndexedSegmentReader;
#[cfg(feature = "wal-cursor")]
pub use features::wal_cursor::WalCursorReader;
#[cfg(feature = "xxh3")]
pub use features::xxh3::Xxh3SegmentReader;

#[cfg(test)]
#[path = "segment_tests.rs"]
mod segment_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
