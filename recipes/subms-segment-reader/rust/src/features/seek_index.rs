//! Sparse seek index for random-access reads.
//!
//! `IndexedSegmentReader` walks the segment once at open time, recording
//! the byte offset of every Nth block (default N=64) into an in-memory
//! `Vec<(block_idx, byte_offset)>`. `seek_to_block(target)` then binary-
//! searches the index to find the indexed block at-or-before the target
//! and scans forward at most N-1 blocks.
//!
//! Cost: one pass at open; ~16 bytes per indexed block held in RAM.
//! Benefit: random-access seek becomes O(log N) + a bounded linear
//! tail, vs O(N) by scanning from the head.

use crate::Error;

/// Index a sparse block every `INDEX_STRIDE`-th block. 64 is the
/// default in LevelDB / RocksDB SSTable footers; small enough to keep
/// the post-index scan tight, big enough that the index itself is a
/// rounding-error fraction of the file.
pub const INDEX_STRIDE: u32 = 64;

#[derive(Debug, Clone, Copy)]
pub struct IndexEntry {
    pub block_idx: u32,
    pub byte_offset: usize,
}

pub struct IndexedSegmentReader<'a> {
    buf: &'a [u8],
    index: Vec<IndexEntry>,
    /// Total block count discovered during the open-time scan.
    total_blocks: u32,
    /// Current cursor into `buf`.
    pos: usize,
    /// Current block-index cursor.
    cur_block: u32,
}

impl<'a> IndexedSegmentReader<'a> {
    /// Build a sparse index over `buf`. Returns `Err(TruncatedFrame)`
    /// if the segment is corrupted; downstream code can still scan
    /// up to the indexed offsets even if the tail is bad.
    pub fn open(buf: &'a [u8]) -> Result<Self, Error> {
        let mut index = Vec::new();
        let mut pos = 0;
        let mut block_idx: u32 = 0;
        while pos < buf.len() {
            if block_idx % INDEX_STRIDE == 0 {
                index.push(IndexEntry {
                    block_idx,
                    byte_offset: pos,
                });
            }
            if pos + 4 > buf.len() {
                return Err(Error::TruncatedFrame);
            }
            let len = u32::from_be_bytes(buf[pos..pos + 4].try_into().unwrap()) as usize;
            if pos + 4 + len > buf.len() {
                return Err(Error::TruncatedFrame);
            }
            pos += 4 + len;
            block_idx = block_idx.checked_add(1).expect("block index overflow");
        }
        Ok(Self {
            buf,
            index,
            total_blocks: block_idx,
            pos: 0,
            cur_block: 0,
        })
    }

    /// Number of blocks (records) in the segment.
    pub fn total_blocks(&self) -> u32 {
        self.total_blocks
    }

    /// Number of entries in the sparse index. Roughly
    /// `(total_blocks + INDEX_STRIDE - 1) / INDEX_STRIDE`.
    pub fn index_len(&self) -> usize {
        self.index.len()
    }

    /// Position the reader so the next `next_record()` returns
    /// block `target`. Returns `Err(TruncatedFrame)` only if the index
    /// itself was constructed against a corrupted segment; out-of-
    /// range targets land cleanly at the EOF marker (next_record None).
    pub fn seek_to_block(&mut self, target: u32) -> Result<(), Error> {
        if target >= self.total_blocks {
            // Seek past the end - position at EOF.
            self.pos = self.buf.len();
            self.cur_block = self.total_blocks;
            return Ok(());
        }
        // Binary search for the index entry at-or-before `target`.
        let idx = match self.index.binary_search_by_key(&target, |e| e.block_idx) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let entry = self.index[idx];
        self.pos = entry.byte_offset;
        self.cur_block = entry.block_idx;
        // Linear-scan forward from the index entry up to the target.
        while self.cur_block < target {
            if self.pos + 4 > self.buf.len() {
                return Err(Error::TruncatedFrame);
            }
            let len =
                u32::from_be_bytes(self.buf[self.pos..self.pos + 4].try_into().unwrap()) as usize;
            self.pos += 4 + len;
            self.cur_block += 1;
        }
        Ok(())
    }

    /// Read the next record. Returns `Ok(None)` when the cursor is
    /// at EOF; `Err(TruncatedFrame)` should not happen during normal
    /// use because `open()` already validated the framing.
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
        self.cur_block += 1;
        Ok(Some(&self.buf[payload_start..payload_end]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SegmentWriter;

    fn build_n(n: u32) -> Vec<u8> {
        let mut buf = Vec::new();
        let mut w = SegmentWriter::new(&mut buf);
        for i in 0..n {
            w.write(format!("rec-{i}").as_bytes()).unwrap();
        }
        buf
    }

    #[test]
    fn index_stride_matches_block_layout() {
        let buf = build_n(200);
        let r = IndexedSegmentReader::open(&buf).unwrap();
        assert_eq!(r.total_blocks(), 200);
        // 200 blocks at stride 64 -> indexes at 0, 64, 128, 192 = 4 entries.
        assert_eq!(r.index_len(), 4);
    }

    #[test]
    fn seek_to_first_block() {
        let buf = build_n(200);
        let mut r = IndexedSegmentReader::open(&buf).unwrap();
        r.seek_to_block(0).unwrap();
        assert_eq!(r.next_record().unwrap().unwrap(), b"rec-0");
    }

    #[test]
    fn seek_to_indexed_block() {
        let buf = build_n(200);
        let mut r = IndexedSegmentReader::open(&buf).unwrap();
        r.seek_to_block(128).unwrap();
        assert_eq!(r.next_record().unwrap().unwrap(), b"rec-128");
    }

    #[test]
    fn seek_to_unindexed_block_scans_forward() {
        let buf = build_n(200);
        let mut r = IndexedSegmentReader::open(&buf).unwrap();
        // Block 100: nearest entry below is block 64 (entry idx 1).
        r.seek_to_block(100).unwrap();
        assert_eq!(r.next_record().unwrap().unwrap(), b"rec-100");
    }

    #[test]
    fn seek_past_end_yields_none() {
        let buf = build_n(50);
        let mut r = IndexedSegmentReader::open(&buf).unwrap();
        r.seek_to_block(9999).unwrap();
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn open_on_corrupted_tail_errors() {
        let mut buf = build_n(10);
        buf.extend_from_slice(&[0, 0, 0]); // truncated header
        assert!(matches!(
            IndexedSegmentReader::open(&buf),
            Err(Error::TruncatedFrame)
        ));
    }

    #[test]
    fn sequential_next_record_walks_every_block() {
        let buf = build_n(40);
        let mut r = IndexedSegmentReader::open(&buf).unwrap();
        for i in 0..40u32 {
            let got = r.next_record().unwrap().unwrap();
            assert_eq!(got, format!("rec-{i}").as_bytes());
        }
        assert!(r.next_record().unwrap().is_none());
    }

    #[test]
    fn empty_segment_index_is_empty() {
        let buf: Vec<u8> = Vec::new();
        let r = IndexedSegmentReader::open(&buf).unwrap();
        assert_eq!(r.total_blocks(), 0);
        assert_eq!(r.index_len(), 0);
    }
}
