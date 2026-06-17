package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;

import java.io.IOException;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;

/**
 * Sparse seek index for random-access reads.
 *
 * <p>Walks the segment once at open time, recording the byte offset of
 * every Nth block (default N=64) into an in-memory list.
 * {@link #seekToBlock(int)} then binary-searches the index to find the
 * entry at-or-before the target and scans forward at most N-1 blocks.
 *
 * <p>Cost: one pass at open; ~16 bytes per indexed block held in RAM.
 * Benefit: random-access seek becomes O(log N) plus a bounded linear
 * tail, vs O(N) by scanning from the head.
 */
public final class IndexedSegmentReader {

    /** Index every {@code INDEX_STRIDE}th block. 64 matches LevelDB /
     *  RocksDB SSTable footers. */
    public static final int INDEX_STRIDE = 64;

    public static final class IndexEntry {
        public final int blockIdx;
        public final int byteOffset;
        IndexEntry(int blockIdx, int byteOffset) {
            this.blockIdx = blockIdx;
            this.byteOffset = byteOffset;
        }
    }

    private final ByteBuffer buf;
    private final List<IndexEntry> index;
    private final int totalBlocks;
    private int curBlock;

    public IndexedSegmentReader(byte[] bytes) throws IOException {
        this.buf = ByteBuffer.wrap(bytes);
        this.index = new ArrayList<>();
        int pos = 0;
        int blockIdx = 0;
        while (pos < bytes.length) {
            if (blockIdx % INDEX_STRIDE == 0) {
                index.add(new IndexEntry(blockIdx, pos));
            }
            if (pos + 4 > bytes.length) {
                throw new SegmentReader.TruncatedFrame("header truncated at block " + blockIdx);
            }
            int len = ((bytes[pos] & 0xff) << 24)
                    | ((bytes[pos + 1] & 0xff) << 16)
                    | ((bytes[pos + 2] & 0xff) << 8)
                    | (bytes[pos + 3] & 0xff);
            if (len < 0) {
                throw new IOException("negative frame length at block " + blockIdx + ": " + len);
            }
            if (pos + 4 + len > bytes.length) {
                throw new SegmentReader.TruncatedFrame("payload truncated at block " + blockIdx);
            }
            pos += 4 + len;
            blockIdx++;
        }
        this.totalBlocks = blockIdx;
        this.buf.position(0);
        this.curBlock = 0;
    }

    public int totalBlocks() { return totalBlocks; }
    public int indexLen() { return index.size(); }

    /** Position the reader so the next {@link #nextRecord()} returns
     *  block {@code target}. Out-of-range targets land cleanly at EOF. */
    public void seekToBlock(int target) {
        if (target >= totalBlocks) {
            buf.position(buf.capacity());
            curBlock = totalBlocks;
            return;
        }
        int lo = 0, hi = index.size();
        // Binary search for the largest entry with blockIdx <= target.
        while (lo < hi) {
            int mid = (lo + hi) >>> 1;
            if (index.get(mid).blockIdx <= target) lo = mid + 1; else hi = mid;
        }
        IndexEntry entry = index.get(lo - 1);
        buf.position(entry.byteOffset);
        curBlock = entry.blockIdx;
        while (curBlock < target) {
            int len = buf.getInt();
            buf.position(buf.position() + len);
            curBlock++;
        }
    }

    /** Read the next record. Returns {@code null} at EOF. */
    public byte[] nextRecord() {
        if (!buf.hasRemaining()) return null;
        int len = buf.getInt();
        byte[] out = new byte[len];
        buf.get(out);
        curBlock++;
        return out;
    }
}
