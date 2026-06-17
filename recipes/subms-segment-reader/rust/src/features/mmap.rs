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
mod tests {
    use super::*;
    use crate::SegmentWriter;
    use std::fs;
    use std::io::Write;
    use std::path::PathBuf;

    fn temp_path(label: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        p.push(format!(
            "subms-segment-mmap-{label}-{stamp}-{}.bin",
            std::process::id()
        ));
        p
    }

    fn write_segment(path: &Path, records: &[&[u8]]) {
        let mut f = fs::File::create(path).unwrap();
        let mut w = SegmentWriter::new(&mut f);
        for r in records {
            w.write(r).unwrap();
        }
        f.flush().unwrap();
    }

    #[test]
    fn round_trip_via_mmap() {
        let path = temp_path("round-trip");
        write_segment(&path, &[b"alice", b"bob", b"carol"]);
        let mut r = MmapSegmentReader::open(&path).unwrap();
        assert_eq!(r.next_record().unwrap().unwrap(), b"alice");
        assert_eq!(r.next_record().unwrap().unwrap(), b"bob");
        assert_eq!(r.next_record().unwrap().unwrap(), b"carol");
        assert!(r.next_record().unwrap().is_none());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn empty_file_yields_none() {
        let path = temp_path("empty");
        fs::write(&path, b"").unwrap();
        let mut r = MmapSegmentReader::open(&path).unwrap();
        assert_eq!(r.len(), 0);
        assert!(r.is_empty());
        assert!(r.next_record().unwrap().is_none());
        fs::remove_file(&path).ok();
    }

    #[test]
    fn missing_file_returns_io_error() {
        let path = temp_path("missing");
        match MmapSegmentReader::open(&path) {
            Err(e) => assert_eq!(e.kind(), io::ErrorKind::NotFound),
            Ok(_) => panic!("expected NotFound, got Ok"),
        }
    }

    #[test]
    fn truncated_header_surfaces_typed_error() {
        let path = temp_path("trunc-header");
        let mut bytes = Vec::new();
        {
            let mut w = SegmentWriter::new(&mut bytes);
            w.write(b"first").unwrap();
        }
        bytes.extend_from_slice(&[0x00, 0x00]); // 2 bytes of a 4-byte header
        fs::write(&path, &bytes).unwrap();
        let mut r = MmapSegmentReader::open(&path).unwrap();
        assert_eq!(r.next_record().unwrap().unwrap(), b"first");
        assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn truncated_payload_surfaces_typed_error() {
        let path = temp_path("trunc-payload");
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&[0x00, 0x00, 0x00, 0x0a]); // claim 10 bytes
        bytes.extend_from_slice(b"abc"); // only 3 actually
        fs::write(&path, &bytes).unwrap();
        let mut r = MmapSegmentReader::open(&path).unwrap();
        assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
        fs::remove_file(&path).ok();
    }

    #[test]
    fn rewind_replays_from_start() {
        let path = temp_path("rewind");
        write_segment(&path, &[b"one", b"two"]);
        let mut r = MmapSegmentReader::open(&path).unwrap();
        assert_eq!(r.next_record().unwrap().unwrap(), b"one");
        r.rewind();
        assert_eq!(r.next_record().unwrap().unwrap(), b"one");
        assert_eq!(r.next_record().unwrap().unwrap(), b"two");
        fs::remove_file(&path).ok();
    }

    #[test]
    fn as_bytes_returns_full_mapping() {
        let path = temp_path("as-bytes");
        write_segment(&path, &[b"xyz"]);
        let r = MmapSegmentReader::open(&path).unwrap();
        // 4 byte header + 3 byte payload.
        assert_eq!(r.as_bytes().len(), 7);
        assert_eq!(r.len(), 7);
        assert!(!r.is_empty());
        fs::remove_file(&path).ok();
    }
}
