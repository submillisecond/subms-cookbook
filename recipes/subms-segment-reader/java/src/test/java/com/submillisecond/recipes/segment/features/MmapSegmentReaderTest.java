package com.submillisecond.recipes.segment.features;

import com.submillisecond.recipes.segment.SegmentReader;
import com.submillisecond.recipes.segment.SegmentWriter;
import org.junit.jupiter.api.Test;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.NoSuchFileException;
import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class MmapSegmentReaderTest {

    private static Path writeTempSegment(String label, byte[]... records) throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        SegmentWriter w = new SegmentWriter(dos);
        for (byte[] r : records) w.write(r);
        dos.close();
        Path p = Files.createTempFile("subms-segment-mmap-" + label + "-", ".bin");
        Files.write(p, baos.toByteArray());
        return p;
    }

    @Test
    void roundTripViaMmap() throws IOException {
        Path p = writeTempSegment("round-trip", "alice".getBytes(), "bob".getBytes(), "carol".getBytes());
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            assertArrayEquals("alice".getBytes(), r.nextRecord());
            assertArrayEquals("bob".getBytes(), r.nextRecord());
            assertArrayEquals("carol".getBytes(), r.nextRecord());
            assertNull(r.nextRecord());
        } finally {
            Files.deleteIfExists(p);
        }
    }

    @Test
    void emptyFileYieldsNull() throws IOException {
        Path p = Files.createTempFile("subms-segment-mmap-empty-", ".bin");
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            assertEquals(0L, r.length());
            assertTrue(r.isEmpty());
            assertNull(r.nextRecord());
        } finally {
            Files.deleteIfExists(p);
        }
    }

    @Test
    void missingFileThrows() {
        Path p = Path.of(System.getProperty("java.io.tmpdir"),
                "subms-segment-mmap-missing-" + System.nanoTime() + ".bin");
        assertThrows(NoSuchFileException.class, () -> new MmapSegmentReader(p));
    }

    @Test
    void truncatedHeaderSurfacesTypedError() throws IOException {
        ByteArrayOutputStream baos = new ByteArrayOutputStream();
        DataOutputStream dos = new DataOutputStream(baos);
        new SegmentWriter(dos).write("first".getBytes());
        dos.close();
        byte[] body = baos.toByteArray();
        byte[] hand = new byte[body.length + 2];
        System.arraycopy(body, 0, hand, 0, body.length);
        // 2 of 4 trailing header bytes left as zero.
        Path p = Files.createTempFile("subms-segment-mmap-trunc-hdr-", ".bin");
        Files.write(p, hand);
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            assertArrayEquals("first".getBytes(), r.nextRecord());
            assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
        } finally {
            Files.deleteIfExists(p);
        }
    }

    @Test
    void truncatedPayloadSurfacesTypedError() throws IOException {
        byte[] hand = new byte[]{0, 0, 0, 10, 'a', 'b', 'c'}; // header says 10 but only 3 follow
        Path p = Files.createTempFile("subms-segment-mmap-trunc-payload-", ".bin");
        Files.write(p, hand);
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            assertThrows(SegmentReader.TruncatedFrame.class, r::nextRecord);
        } finally {
            Files.deleteIfExists(p);
        }
    }

    @Test
    void rewindReplaysFromStart() throws IOException {
        Path p = writeTempSegment("rewind", "one".getBytes(), "two".getBytes());
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            assertArrayEquals("one".getBytes(), r.nextRecord());
            r.rewind();
            assertArrayEquals("one".getBytes(), r.nextRecord());
            assertArrayEquals("two".getBytes(), r.nextRecord());
        } finally {
            Files.deleteIfExists(p);
        }
    }

    @Test
    void lengthReportsFileSize() throws IOException {
        Path p = writeTempSegment("len", "xyz".getBytes());
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            // 4 byte header + 3 byte payload.
            assertEquals(7L, r.length());
            assertEquals(false, r.isEmpty());
        } finally {
            Files.deleteIfExists(p);
        }
    }

    @Test
    void negativeFrameLengthRejected() throws IOException {
        byte[] hand = new byte[]{(byte) 0xff, (byte) 0xff, (byte) 0xff, (byte) 0xff};
        Path p = Files.createTempFile("subms-segment-mmap-neg-", ".bin");
        Files.write(p, hand);
        try (MmapSegmentReader r = new MmapSegmentReader(p)) {
            assertThrows(IOException.class, r::nextRecord);
        } finally {
            Files.deleteIfExists(p);
        }
    }
}
