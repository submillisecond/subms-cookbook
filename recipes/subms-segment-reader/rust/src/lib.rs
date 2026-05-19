//! Read length-prefix framed records from a segment file.
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
//!
//! ```
//! use subms_segment_reader::{SegmentWriter, SegmentReader};
//! let mut buf = Vec::new();
//! { let mut w = SegmentWriter::new(&mut buf); w.write(b"alice").unwrap(); w.write(b"bob").unwrap(); }
//! let mut r = SegmentReader::new(buf.as_slice());
//! assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
//! assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
//! assert!(r.next_record().unwrap().is_none());
//! ```

use std::io::{self, Read, Write};

#[derive(Debug)]
pub enum Error {
    /// Underlying IO error.
    Io(io::Error),
    /// Header or payload truncated at the tail of the segment.
    TruncatedFrame,
}

impl From<io::Error> for Error {
    fn from(e: io::Error) -> Self { Error::Io(e) }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::TruncatedFrame => write!(f, "truncated frame at segment tail"),
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
        Self { reader, buffer: Vec::new() }
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
