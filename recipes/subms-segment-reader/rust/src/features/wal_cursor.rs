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
#[path = "wal_cursor_tests.rs"]
mod tests;
