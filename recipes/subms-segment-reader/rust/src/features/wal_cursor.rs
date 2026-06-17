//! WAL-style cursor reader.
//!
//! Wraps a byte-slice segment and tracks a `committed_offset` byte
//! watermark. `read_committed()` returns the next block only if its
//! tail lies at or before the watermark - readers see exactly the
//! prefix the writer last fsync'd, never partial post-fsync data.
//!
//! Producer flow:
//!
//! ```ignore
//! writer.write(...)?;
//! writer.write(...)?;
//! writer.flush()?;                  // fsync underlying file
//! reader.set_committed(writer_pos);  // advance the watermark
//! ```
//!
//! Reader flow: call `read_committed()` in a loop; once it returns
//! `Ok(None)` the reader has caught up to the watermark - either wait
//! for the producer to advance, or call `next_record()` (which ignores
//! the watermark) for dirty reads.

use crate::Error;

pub struct WalCursorReader<'a> {
    buf: &'a [u8],
    /// Current read cursor (byte offset into `buf`).
    pos: usize,
    /// Watermark - reads of blocks whose tails exceed this offset are
    /// blocked by `read_committed()` and return `Ok(None)`.
    committed: usize,
}

impl<'a> WalCursorReader<'a> {
    /// Open against `buf` with the watermark at zero (nothing committed).
    pub fn new(buf: &'a [u8]) -> Self {
        Self {
            buf,
            pos: 0,
            committed: 0,
        }
    }

    /// Open against `buf` with the watermark already set. Useful for
    /// readers that know the writer's fsync position up front.
    pub fn with_committed(buf: &'a [u8], committed: usize) -> Self {
        Self {
            buf,
            pos: 0,
            committed: committed.min(buf.len()),
        }
    }

    /// Move the committed-offset watermark forward. Backward moves are
    /// rejected silently - watermarks are monotonic by contract.
    pub fn set_committed(&mut self, offset: usize) {
        let clamped = offset.min(self.buf.len());
        if clamped > self.committed {
            self.committed = clamped;
        }
    }

    pub fn committed(&self) -> usize {
        self.committed
    }

    pub fn position(&self) -> usize {
        self.pos
    }

    /// Read the next block iff its tail lies at or before the
    /// committed watermark. Returns `Ok(None)` at EOF OR when the next
    /// block would step past the watermark - the caller can't tell
    /// the difference, which is the design: durability-aware readers
    /// treat both as "wait".
    pub fn read_committed(&mut self) -> Result<Option<&[u8]>, Error> {
        if self.pos == self.buf.len() {
            return Ok(None);
        }
        if self.pos + 4 > self.buf.len() {
            return Err(Error::TruncatedFrame);
        }
        let len = u32::from_be_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap()) as usize;
        let payload_start = self.pos + 4;
        let payload_end = payload_start + len;
        if payload_end > self.buf.len() {
            return Err(Error::TruncatedFrame);
        }
        if payload_end > self.committed {
            return Ok(None);
        }
        self.pos = payload_end;
        Ok(Some(&self.buf[payload_start..payload_end]))
    }

    /// Dirty read - ignore the watermark and read the next block.
    /// Use for crash-recovery / forensic paths; production replay
    /// should stick with `read_committed()`.
    pub fn next_record(&mut self) -> Result<Option<&[u8]>, Error> {
        if self.pos == self.buf.len() {
            return Ok(None);
        }
        if self.pos + 4 > self.buf.len() {
            return Err(Error::TruncatedFrame);
        }
        let len = u32::from_be_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap()) as usize;
        let payload_start = self.pos + 4;
        let payload_end = payload_start + len;
        if payload_end > self.buf.len() {
            return Err(Error::TruncatedFrame);
        }
        self.pos = payload_end;
        Ok(Some(&self.buf[payload_start..payload_end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SegmentWriter;

    fn build(records: &[&[u8]]) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut w = SegmentWriter::new(&mut buf);
        for r in records {
            w.write(r).unwrap();
        }
        buf
    }

    /// Locate the byte offset of the tail of block `n` (0-indexed) in
    /// a segment built by `build()` - used as a watermark target.
    fn end_of_block(buf: &[u8], n: usize) -> usize {
        let mut pos = 0usize;
        for _ in 0..=n {
            let len = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
            pos += 4 + len;
        }
        pos
    }

    #[test]
    fn nothing_visible_until_watermark_advances() {
        let buf = build(&[b"a", b"b", b"c"]);
        let mut r = WalCursorReader::new(&buf);
        assert!(
            r.read_committed().unwrap().is_none(),
            "watermark at 0 -> nothing"
        );
    }

    #[test]
    fn advance_to_first_block_exposes_first_block_only() {
        let buf = build(&[b"a", b"b", b"c"]);
        let mut r = WalCursorReader::new(&buf);
        r.set_committed(end_of_block(&buf, 0));
        assert_eq!(r.read_committed().unwrap().unwrap(), b"a");
        assert!(
            r.read_committed().unwrap().is_none(),
            "second block not yet committed"
        );
    }

    #[test]
    fn advance_again_exposes_more_blocks() {
        let buf = build(&[b"a", b"b", b"c"]);
        let mut r = WalCursorReader::new(&buf);
        r.set_committed(end_of_block(&buf, 0));
        let _ = r.read_committed().unwrap();
        r.set_committed(end_of_block(&buf, 2));
        assert_eq!(r.read_committed().unwrap().unwrap(), b"b");
        assert_eq!(r.read_committed().unwrap().unwrap(), b"c");
        assert!(r.read_committed().unwrap().is_none());
    }

    #[test]
    fn watermark_is_monotonic() {
        let buf = build(&[b"a", b"b"]);
        let mut r = WalCursorReader::new(&buf);
        r.set_committed(end_of_block(&buf, 1));
        assert_eq!(r.committed(), end_of_block(&buf, 1));
        r.set_committed(0); // attempt to walk it back
        assert_eq!(
            r.committed(),
            end_of_block(&buf, 1),
            "backward moves rejected"
        );
    }

    #[test]
    fn watermark_clamps_to_buffer_length() {
        let buf = build(&[b"a"]);
        let mut r = WalCursorReader::new(&buf);
        r.set_committed(9999);
        assert_eq!(r.committed(), buf.len());
    }

    #[test]
    fn with_committed_seeds_watermark_at_open() {
        let buf = build(&[b"a", b"b"]);
        let mut r = WalCursorReader::with_committed(&buf, end_of_block(&buf, 0));
        assert_eq!(r.read_committed().unwrap().unwrap(), b"a");
        assert!(r.read_committed().unwrap().is_none());
    }

    #[test]
    fn dirty_next_record_ignores_watermark() {
        let buf = build(&[b"a", b"b"]);
        let mut r = WalCursorReader::new(&buf); // committed = 0
        assert_eq!(r.next_record().unwrap().unwrap(), b"a");
        assert_eq!(r.next_record().unwrap().unwrap(), b"b");
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn truncated_tail_surfaces_typed_error() {
        let mut buf = build(&[b"first"]);
        buf.extend_from_slice(&[0, 0, 0, 10]);
        buf.extend_from_slice(b"abc"); // 3 of claimed 10
        let mut r = WalCursorReader::new(&buf);
        r.set_committed(buf.len());
        assert_eq!(r.read_committed().unwrap().unwrap(), b"first");
        assert!(matches!(r.read_committed(), Err(Error::TruncatedFrame)));
    }

    #[test]
    fn position_advances_with_reads() {
        let buf = build(&[b"a", b"bb"]);
        let mut r = WalCursorReader::new(&buf);
        r.set_committed(buf.len());
        assert_eq!(r.position(), 0);
        let _ = r.read_committed().unwrap();
        assert_eq!(r.position(), end_of_block(&buf, 0));
    }
}
