package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;

import java.io.Closeable;
import java.io.IOException;
import java.nio.ByteBuffer;
import java.nio.channels.FileChannel;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;

/**
 * Memory-mapped read path. Maps the segment file into the JVM's
 * address space via {@code FileChannel.map(MapMode.READ_ONLY, ...)} and
 * parses length-prefix frames directly out of the mapped buffer - the
 * OS pages the file in lazily on first access to each page.
 *
 * <p>Trades startup speed (constant-time open, no copy) for not loading
 * the full file into RAM. Pairs well with cold-start replay where
 * random access dominates and the working set is smaller than the file.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_segment_reader::MmapSegmentReader}.
 */
public final class MmapSegmentReader implements Closeable {

    private final FileChannel channel;
    private final ByteBuffer view;
    private final long fileLen;

    public MmapSegmentReader(Path path) throws IOException {
        this.channel = FileChannel.open(path, StandardOpenOption.READ);
        this.fileLen = channel.size();
        if (fileLen == 0) {
            // FileChannel.map rejects 0-length maps; substitute an empty buffer.
            this.view = ByteBuffer.allocate(0);
        } else {
            this.view = channel.map(FileChannel.MapMode.READ_ONLY, 0, fileLen);
        }
    }

    public long length() { return fileLen; }
    public boolean isEmpty() { return fileLen == 0; }

    /** Read the next record. Returns {@code null} at clean EOF;
     *  throws {@link SegmentReader.TruncatedFrame} when the file ends
     *  mid-frame. */
    public byte[] nextRecord() throws IOException {
        if (!view.hasRemaining()) return null;
        if (view.remaining() < 4) throw new SegmentReader.TruncatedFrame("header truncated at tail");
        int len = view.getInt();
        if (len < 0) throw new IOException("negative frame length: " + len);
        if (view.remaining() < len) throw new SegmentReader.TruncatedFrame("payload truncated at tail");
        byte[] out = new byte[len];
        view.get(out);
        return out;
    }

    /** Reset the read cursor to the start of the file. */
    public void rewind() {
        view.position(0);
    }

    @Override
    public void close() throws IOException {
        channel.close();
        // The MappedByteBuffer holds an OS mapping until GC reclaims it.
        // JDK 21 has no public unmap; rely on the channel close + GC.
    }
}
