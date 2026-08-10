package com.submillisecond.recipes.hll;

import com.submillisecond.recipes.hll.features.SparseHyperLogLog;
import org.junit.jupiter.api.Test;

import java.util.Arrays;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class HllCodecTest {

    private static final String[] FIXTURE_KEYS = {"AAPL", "MSFT", "NVDA", "TSLA", "AMZN"};
    private static final int FIXTURE_P = 4;

    /**
     * The same bytes the Rust port pins in {@code codec_tests.rs}. If this
     * changes, the Rust fixture must change with it - and that is a format
     * break, not a refactor.
     */
    private static final String DENSE_FIXTURE_HEX =
        "53484c4c0100040000010000000000000000010001030003";
    private static final String SPARSE_FIXTURE_HEX =
        "53484c4c0101040000000040000000050000000c010000000a010000000f0300000001010000000d03";

    private static String hex(byte[] bytes) {
        StringBuilder sb = new StringBuilder(bytes.length * 2);
        for (byte b : bytes) sb.append(String.format("%02x", b));
        return sb.toString();
    }

    private static HyperLogLog fixture() {
        HyperLogLog hll = new HyperLogLog(FIXTURE_P);
        for (String k : FIXTURE_KEYS) hll.add(k);
        return hll;
    }

    @Test
    void denseWireBytesMatchTheRustFixture() {
        assertEquals(DENSE_FIXTURE_HEX, hex(HllCodec.toBytes(fixture())));
    }

    @Test
    void sparseWireBytesMatchTheRustFixture() {
        SparseHyperLogLog s = new SparseHyperLogLog(FIXTURE_P, 64);
        for (String k : FIXTURE_KEYS) s.add(k);
        assertTrue(s.isSparse());
        assertEquals(SPARSE_FIXTURE_HEX, hex(HllCodec.toBytes(s)));
    }

    @Test
    void denseRoundTripPreservesRegisters() {
        HyperLogLog hll = fixture();
        HyperLogLog back = HllCodec.fromBytes(HllCodec.toBytes(hll));
        assertEquals(hll.precision(), back.precision());
        assertArrayEquals(hll.registers(), back.registers());
        assertEquals(hll.estimate(), back.estimate());
    }

    @Test
    void denseRoundTripAtWorkingPrecision() {
        HyperLogLog hll = new HyperLogLog(14);
        for (long i = 0; i < 20_000; i++) hll.addLong(i);
        byte[] bytes = HllCodec.toBytes(hll);
        assertEquals(8 + 16_384, bytes.length);
        assertEquals(hll.estimate(), HllCodec.fromBytes(bytes).estimate());
    }

    @Test
    void rejectsForeignMagic() {
        byte[] bytes = HllCodec.toBytes(fixture());
        bytes[0] = 'X';
        assertEquals(HllException.Kind.BAD_MAGIC,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(bytes)).kind());
    }

    @Test
    void rejectsFutureVersion() {
        byte[] bytes = HllCodec.toBytes(fixture());
        bytes[4] = 2;
        assertEquals(HllException.Kind.UNSUPPORTED_VERSION,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(bytes)).kind());
    }

    @Test
    void rejectsUnknownEncoding() {
        byte[] bytes = HllCodec.toBytes(fixture());
        bytes[5] = 9;
        assertEquals(HllException.Kind.UNSUPPORTED_ENCODING,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(bytes)).kind());
        assertEquals(HllException.Kind.UNSUPPORTED_ENCODING,
            assertThrows(HllException.class, () -> HllCodec.sparseFromBytes(bytes)).kind());
    }

    @Test
    void rejectsOutOfRangePrecision() {
        byte[] bytes = HllCodec.toBytes(fixture());
        bytes[6] = 31;
        assertEquals(HllException.Kind.INVALID_PRECISION,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(bytes)).kind());
    }

    @Test
    void rejectsTruncatedHeaderAndPayload() {
        byte[] bytes = HllCodec.toBytes(fixture());
        byte[] head = Arrays.copyOf(bytes, 3);
        assertEquals(HllException.Kind.TRUNCATED,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(head)).kind());
        byte[] cut = Arrays.copyOf(bytes, 12);
        assertEquals(HllException.Kind.TRUNCATED,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(cut)).kind());
    }

    @Test
    void decodedSketchMergesWithALiveOne() {
        HyperLogLog shardA = new HyperLogLog(12);
        HyperLogLog shardB = new HyperLogLog(12);
        for (long i = 0; i < 4_000; i++) {
            shardA.addLong(i);
            shardB.addLong(i + 2_000);
        }
        // Only the bytes cross the boundary, never the ids.
        shardA.merge(HllCodec.fromBytes(HllCodec.toBytes(shardB)));
        double est = shardA.estimate();
        assertTrue(Math.abs(est - 6_000.0) / 6_000.0 < 0.05,
            "6000 distinct across two shards, got " + est);
    }

    @Test
    void sparseRoundTripKeepsShapeAndThreshold() {
        SparseHyperLogLog s = new SparseHyperLogLog(12, 64);
        for (long i = 0; i < 30; i++) s.addLong(i);
        SparseHyperLogLog back = HllCodec.sparseFromBytes(HllCodec.toBytes(s));
        assertTrue(back.isSparse());
        assertEquals(64, back.threshold());
        assertEquals(s.entryCount(), back.entryCount());
        assertEquals(s.estimate(), back.estimate());
    }

    @Test
    void promotedSparseWritesTheDenseEncoding() {
        SparseHyperLogLog s = new SparseHyperLogLog(10, 8);
        for (long i = 0; i < 50; i++) s.addLong(i);
        assertFalse(s.isSparse());
        byte[] bytes = HllCodec.toBytes(s);
        assertEquals(0, bytes[5], "dense encoding byte");
        SparseHyperLogLog back = HllCodec.sparseFromBytes(bytes);
        assertFalse(back.isSparse(), "a promoted writer yields a promoted reader");
        assertEquals(s.estimate(), back.estimate());
    }

    @Test
    void sparseReaderRejectsATruncatedEntryList() {
        SparseHyperLogLog s = new SparseHyperLogLog(12, 64);
        for (long i = 0; i < 10; i++) s.addLong(i);
        byte[] bytes = HllCodec.toBytes(s);
        byte[] cut = Arrays.copyOf(bytes, bytes.length - 3);
        assertEquals(HllException.Kind.TRUNCATED,
            assertThrows(HllException.class, () -> HllCodec.sparseFromBytes(cut)).kind());
    }

    @Test
    void denseReaderRefusesASparseBuffer() {
        SparseHyperLogLog s = new SparseHyperLogLog(12, 64);
        s.add("one");
        byte[] bytes = HllCodec.toBytes(s);
        assertEquals(HllException.Kind.UNSUPPORTED_ENCODING,
            assertThrows(HllException.class, () -> HllCodec.fromBytes(bytes)).kind());
    }
}
