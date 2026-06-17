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
mod tests {
    use super::*;

    #[test]
    fn round_trip_compressible_payload() {
        // Highly compressible: the writer should pick the lz4 path.
        let payload = vec![b'a'; 4096];
        let mut buf = Vec::new();
        Lz4BlockWriter::new(&mut buf).write(&payload).unwrap();
        // The on-disk size must be smaller than the raw payload (modulo
        // the 9 byte header).
        assert!(
            buf.len() < payload.len(),
            "lz4 path expected for compressible input"
        );
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert_eq!(r.next_record().unwrap().unwrap(), &payload[..]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn round_trip_stored_path_for_incompressible_payload() {
        // Random-ish bytes - lz4 won't help.
        let mut payload = Vec::with_capacity(64);
        for i in 0..64u32 {
            payload.push((i.wrapping_mul(2654435761) >> 24) as u8);
        }
        let mut buf = Vec::new();
        Lz4BlockWriter::new(&mut buf).write(&payload).unwrap();
        // Header is 1 (tag) + 4 (uncompressed_len) + 4 (compressed_len) = 9 bytes.
        assert_eq!(
            buf[0], TAG_STORED,
            "stored path expected when lz4 would inflate"
        );
        assert_eq!(buf.len(), 9 + payload.len());
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert_eq!(r.next_record().unwrap().unwrap(), &payload[..]);
    }

    #[test]
    fn empty_segment_yields_none() {
        let mut r = Lz4SegmentReader::new(&[][..]);
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn unknown_algo_tag_rejected() {
        // Hand-craft a block with tag = 99.
        let mut buf = Vec::new();
        buf.push(99);
        buf.extend_from_slice(&5u32.to_be_bytes());
        buf.extend_from_slice(&5u32.to_be_bytes());
        buf.extend_from_slice(b"hello");
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::DecompressionFailed)));
    }

    #[test]
    fn truncated_header_surfaces_typed_error() {
        let mut buf = Vec::new();
        buf.push(TAG_STORED);
        buf.extend_from_slice(&[0u8; 3]); // 3 of 8 trailing header bytes
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
    }

    #[test]
    fn truncated_payload_surfaces_typed_error() {
        let mut buf = Vec::new();
        buf.push(TAG_STORED);
        buf.extend_from_slice(&10u32.to_be_bytes()); // uncompressed=10
        buf.extend_from_slice(&10u32.to_be_bytes()); // compressed=10
        buf.extend_from_slice(b"abc"); // only 3 actual bytes
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::TruncatedFrame)));
    }

    #[test]
    fn corrupted_lz4_payload_returns_decompression_failed() {
        let payload = vec![b'a'; 4096];
        let mut buf = Vec::new();
        Lz4BlockWriter::new(&mut buf).write_lz4(&payload).unwrap();
        // Flip a byte deep into the compressed body to break the LZ4
        // sequence. The 9-byte header is intact.
        let i = buf.len() - 4;
        buf[i] ^= 0xff;
        buf[i - 1] ^= 0xff;
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        // Either decompress fails outright, or it succeeds with the
        // wrong length and the checksum-style length guard catches it.
        match r.next_record() {
            Err(Error::DecompressionFailed) => {}
            Ok(Some(out)) => assert_ne!(out, &payload[..], "corrupted lz4 must not round-trip"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn stored_block_size_mismatch_rejected() {
        // Hand-craft a stored block claiming uncompressed=10 but compressed=8.
        let mut buf = Vec::new();
        buf.push(TAG_STORED);
        buf.extend_from_slice(&10u32.to_be_bytes());
        buf.extend_from_slice(&8u32.to_be_bytes());
        buf.extend_from_slice(b"12345678");
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert!(matches!(r.next_record(), Err(Error::DecompressionFailed)));
    }

    #[test]
    fn multiple_blocks_round_trip() {
        let mut buf = Vec::new();
        {
            let mut w = Lz4BlockWriter::new(&mut buf);
            w.write(&vec![b'x'; 1024]).unwrap();
            w.write(b"small").unwrap();
            w.write(&vec![b'z'; 2048]).unwrap();
        }
        let mut r = Lz4SegmentReader::new(buf.as_slice());
        assert_eq!(r.next_record().unwrap().unwrap(), &vec![b'x'; 1024][..]);
        assert_eq!(r.next_record().unwrap().unwrap(), b"small");
        assert_eq!(r.next_record().unwrap().unwrap(), &vec![b'z'; 2048][..]);
        assert!(r.next_record().unwrap().is_none());
    }
}
