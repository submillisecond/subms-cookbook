//! Memory-mapped read path. `MmapSegmentReader` maps the segment file
//! into the process address space via `memmap2::Mmap` and parses
//! length-prefix frames directly out of the mapped slice - the OS pages
//! the file in lazily on first access to each page.
//!
//! Trades startup speed (constant-time open, no copy) for not loading
//! the full file into RAM. Pairs well with cold-start replay where
//! random access dominates and the working set is smaller than the file.

use std::fs::File;
use std::io;
use std::path::Path;

use memmap2::Mmap;

use crate::Error;

/// Mapped or zero-length-stub backing for the reader. Some platforms
/// refuse mmap on a 0-byte file; we handle that by holding no map at all.
enum Backing {
    Mapped(Mmap),
    Empty,
}

impl Backing {
    fn as_slice(&self) -> &[u8] {
        match self {
            Backing::Mapped(m) => &m[..],
            Backing::Empty => &[],
        }
    }
}

pub struct MmapSegmentReader {
    backing: Backing,
    pos: usize,
}

impl MmapSegmentReader {
    /// Open and mmap the file at `path`. Returns an error if the file
    /// is missing or the mmap call fails. Zero-length files are
    /// accepted explicitly (some platforms refuse to mmap them).
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        if len == 0 {
            return Ok(Self {
                backing: Backing::Empty,
                pos: 0,
            });
        }
        // SAFETY: the file is opened read-only and the lifetime of the
        // Mmap is bounded by this struct. The slice is not exposed
        // mutably and the underlying File handle isn't shared.
        let map = unsafe { Mmap::map(&file)? };
        Ok(Self {
            backing: Backing::Mapped(map),
            pos: 0,
        })
    }

    /// Total bytes of the mapped file.
    pub fn len(&self) -> usize {
        self.backing.as_slice().len()
    }

    /// Whether the file is zero-length.
    pub fn is_empty(&self) -> bool {
        self.backing.as_slice().is_empty()
    }

    /// Borrow the raw mapped bytes. Useful for callers that want to
    /// scan their own framing.
    pub fn as_bytes(&self) -> &[u8] {
        self.backing.as_slice()
    }

    /// Read the next record. Returns `Ok(None)` at clean EOF;
    /// `Err(Error::TruncatedFrame)` if the file ends mid-frame.
    pub fn next_record(&mut self) -> Result<Option<&[u8]>, Error> {
        let buf = self.backing.as_slice();
        if self.pos == buf.len() {
            return Ok(None);
        }
        if self.pos + 4 > buf.len() {
            return Err(Error::TruncatedFrame);
        }
        let len = u32::from_be_bytes(buf[self.pos..self.pos + 4].try_into().unwrap()) as usize;
        let payload_start = self.pos + 4;
        let payload_end = payload_start + len;
        if payload_end > buf.len() {
            return Err(Error::TruncatedFrame);
        }
        self.pos = payload_end;
        Ok(Some(&buf[payload_start..payload_end]))
    }

    /// Reset the read cursor to the start of the file.
    pub fn rewind(&mut self) {
        self.pos = 0;
    }
}

#[cfg(test)]
#[path = "mmap_tests.rs"]
mod tests;
