package com.submillisecond.recipes.tsparquet;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.io.EOFException;
import java.nio.ByteBuffer;
import org.apache.parquet.io.PositionOutputStream;
import org.junit.jupiter.api.Test;

class TsParquetTest {

    private static TsSeriesD series(long id, String name, int n) {
        TsSeriesD s = new TsSeriesD();
        long base = 1_780_000_000_000_000_000L;
        for (int i = 0; i < n; i++) {
            s.push(base + (long) i * 1_000_000_000L, i * 0.5);
        }
        return s.withMetadata(
                new TsSeriesMetadata(id, name).withTag("host", "edge-01").withTag("region", "us-east-1"));
    }

    @Test
    void parquetBytesHaveMagicHeader() {
        byte[] b = ParquetConvert.seriesToParquet(series(1, "cpu", 8));
        assertArrayEquals(new byte[] {'P', 'A', 'R', '1'}, new byte[] {b[0], b[1], b[2], b[3]});
        assertArrayEquals(
                new byte[] {'P', 'A', 'R', '1'},
                new byte[] {b[b.length - 4], b[b.length - 3], b[b.length - 2], b[b.length - 1]});
    }

    @Test
    void seriesRoundTripsThroughParquet() {
        TsSeriesD s = series(3, "cpu", 64);
        TsSeriesD back = ParquetConvert.parquetToSeries(ParquetConvert.seriesToParquet(s));
        assertEquals(64, back.size());
        assertEquals("cpu", back.metadata().orElseThrow().name());
        assertEquals(s.last().orElseThrow().value(), back.last().orElseThrow().value());
    }

    @Test
    void parquetPreservesMetadataAndTags() {
        TsSeriesD back =
                ParquetConvert.parquetToSeries(ParquetConvert.seriesToParquet(series(9, "trades.aapl", 4)));
        TsSeriesMetadata m = back.metadata().orElseThrow();
        assertEquals(9, m.id());
        assertEquals("trades.aapl", m.name());
        assertEquals("edge-01", m.tags().get("host"));
        assertEquals("us-east-1", m.tags().get("region"));
    }

    @Test
    void valuesSurviveExactly() {
        TsSeriesD s = new TsSeriesD();
        long[] ts = {1, 2, 3, 4};
        double[] vs = {1.25, -3.5, 1e300, 0.0};
        for (int i = 0; i < 4; i++) {
            s.push(ts[i], vs[i]);
        }
        TsSeriesD back = ParquetConvert.parquetToSeries(ParquetConvert.seriesToParquet(s));
        for (int i = 0; i < 4; i++) {
            assertEquals(vs[i], back.toList().get(i).value());
        }
    }

    @Test
    void emptySeriesRoundTrips() {
        TsSeriesD s = new TsSeriesD().withMetadata(new TsSeriesMetadata(1, "empty"));
        TsSeriesD back = ParquetConvert.parquetToSeries(ParquetConvert.seriesToParquet(s));
        assertEquals(0, back.size());
        assertEquals("empty", back.metadata().orElseThrow().name());
    }

    @Test
    void seriesWithNoMetadataRoundTrips() {
        TsSeriesD s = new TsSeriesD();
        s.push(10, 1.0);
        s.push(20, 2.0);
        TsSeriesD back = ParquetConvert.parquetToSeries(ParquetConvert.seriesToParquet(s));
        assertEquals(2, back.size());
    }

    @Test
    void collectionRoundTripsThroughParquet() {
        TsCollection<Double> coll = new TsCollection<>();
        for (long id = 1; id <= 3; id++) {
            coll.register(new TsSeriesMetadata(id, "s" + id));
            for (int i = 0; i < 5; i++) {
                coll.push(id, 1_000 + i, i * 1.0);
            }
        }
        TsCollection<Double> back =
                ParquetConvert.parquetToCollection(ParquetConvert.collectionToParquet(coll));
        assertEquals(3, back.size());
        assertEquals(5, back.byName("s2").orElseThrow().size());
    }

    @Test
    void emptyCollectionRoundTrips() {
        TsCollection<Double> back =
                ParquetConvert.parquetToCollection(ParquetConvert.collectionToParquet(new TsCollection<>()));
        assertEquals(0, back.size());
    }

    @Test
    void decodeRejectsGarbage() {
        TsParquetException ex = assertThrows(
                TsParquetException.class,
                () -> ParquetConvert.parquetToSeries(new byte[] {1, 2, 3, 4, 5, 6, 7, 8}));
        assertEquals(TsParquetException.Kind.PARQUET, ex.kind());
    }

    @Test
    void largerSeriesRoundTrips() {
        TsSeriesD s = series(1, "cpu", 5_000);
        TsSeriesD back = ParquetConvert.parquetToSeries(ParquetConvert.seriesToParquet(s));
        assertEquals(5_000, back.size());
        assertEquals(s.last().orElseThrow().value(), back.last().orElseThrow().value());
    }

    @Test
    void parquetBeatsNaiveSizeOnRepetitiveData() {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < 2_000; i++) {
            s.push(i, 42.0);
        }
        byte[] b = ParquetConvert.seriesToParquet(s);
        assertTrue(b.length < 32_000, "parquet len " + b.length + " not < 32000");
    }

    @Test
    void exceptionFactoriesCarryKind() {
        assertEquals(TsParquetException.Kind.SCHEMA, TsParquetException.schema("s").kind());
        assertEquals(TsParquetException.Kind.PARQUET, TsParquetException.parquet("p").kind());
    }

    @Test
    void writeOptionsFactoriesAndValidation() {
        TsParquetWriteOptions def = TsParquetWriteOptions.defaults();
        assertTrue(def.pageSize() > 0 && def.rowGroupSize() > 0 && def.dictionary());

        // page sized to the data, clamped to the floor for a tiny series
        assertEquals(TsParquetWriteOptions.MIN_PAGE_SIZE, TsParquetWriteOptions.forPointCount(8).pageSize());
        // a large series scales the page up (8192 points * 16 = 128 KiB > floor)
        assertTrue(TsParquetWriteOptions.forPointCount(8192).pageSize() > TsParquetWriteOptions.MIN_PAGE_SIZE);
        // never above parquet's default page
        assertTrue(TsParquetWriteOptions.forPointCount(1_000_000_000L).pageSize()
                <= org.apache.parquet.hadoop.ParquetWriter.DEFAULT_PAGE_SIZE);

        TsParquetWriteOptions custom = TsParquetWriteOptions.of(32 * 1024, 1024 * 1024)
                .withPageSize(64 * 1024)
                .withRowGroupSize(2L * 1024 * 1024)
                .withDictionary(false);
        assertEquals(64 * 1024, custom.pageSize());
        assertEquals(2L * 1024 * 1024, custom.rowGroupSize());
        assertEquals(false, custom.dictionary());

        assertThrows(IllegalArgumentException.class, () -> new TsParquetWriteOptions(0, 1, true));
        assertThrows(IllegalArgumentException.class, () -> new TsParquetWriteOptions(1, 0, true));
    }

    @Test
    void seriesRoundTripsWithExplicitOptions() {
        TsSeriesD s = series(2, "cpu", 100);
        byte[] bytes = ParquetConvert.seriesToParquet(s, TsParquetWriteOptions.of(8 * 1024, 64 * 1024).withDictionary(false));
        TsSeriesD back = ParquetConvert.parquetToSeries(bytes);
        assertEquals(100, back.size());
        assertEquals("cpu", back.metadata().orElseThrow().name());
        byte[] collBytes = ParquetConvert.collectionToParquet(new TsCollection<>(), TsParquetWriteOptions.defaults());
        assertEquals(0, ParquetConvert.parquetToCollection(collBytes).size());
    }

    @Test
    void inMemoryOutputFileTracksPositionAndOverwrites() throws Exception {
        ParquetConvert.ByteArrayOutputFile of = new ParquetConvert.ByteArrayOutputFile();
        assertEquals(false, of.supportsBlockSize());
        assertEquals(0, of.defaultBlockSize());
        PositionOutputStream ps = of.create(0L);
        ps.write(1);
        ps.write(new byte[] {2, 3});
        ps.write(new byte[] {0, 4, 5, 0}, 1, 2);
        assertEquals(5, ps.getPos());
        ps.close();
        assertArrayEquals(new byte[] {1, 2, 3, 4, 5}, of.toByteArray());

        PositionOutputStream ps2 = of.createOrOverwrite(0L);
        ps2.write(9);
        ps2.close();
        assertArrayEquals(new byte[] {9}, of.toByteArray());
    }

    @Test
    void inMemorySeekableInputStreamReadsEverything() throws Exception {
        byte[] src = {10, 11, 12, 13, 14};
        var in = new ParquetConvert.ByteArraySeekableInputStream(src);
        assertEquals(0, in.getPos());
        assertEquals(10, in.read());
        in.seek(3);
        assertEquals(3, in.getPos());
        assertEquals(13, in.read());

        in.seek(0);
        byte[] two = new byte[2];
        assertEquals(2, in.read(two, 0, 2));
        assertArrayEquals(new byte[] {10, 11}, two);

        byte[] rest = new byte[3];
        in.readFully(rest);
        assertArrayEquals(new byte[] {12, 13, 14}, rest);
        assertEquals(-1, in.read());
        assertEquals(-1, in.read(new byte[1], 0, 1));

        in.seek(0);
        ByteBuffer buf = ByteBuffer.allocate(2);
        assertEquals(2, in.read(buf));
        in.seek(1);
        ByteBuffer buf2 = ByteBuffer.allocate(3);
        in.readFully(buf2);
        in.seek(5);
        assertEquals(-1, in.read(ByteBuffer.allocate(1)));

        var eof = new ParquetConvert.ByteArraySeekableInputStream(src);
        eof.seek(4);
        assertThrows(EOFException.class, () -> eof.readFully(new byte[3]));
        eof.seek(4);
        assertThrows(EOFException.class, () -> eof.readFully(ByteBuffer.allocate(3)));
    }

    @Test
    void collectionValuesAndOrderSurvive() {
        TsCollection<Double> coll = new TsCollection<>();
        coll.register(new TsSeriesMetadata(7, "x"));
        for (int i = 0; i < 10; i++) {
            coll.push(7, 1_000 + i, i * 2.0);
        }
        TsCollection<Double> back =
                ParquetConvert.parquetToCollection(ParquetConvert.collectionToParquet(coll));
        TsSeries<Double> x = back.byName("x").orElseThrow();
        assertEquals(10, x.size());
        double lastV = 0;
        for (TsPoint<Double> p : x) {
            lastV = p.value();
        }
        assertEquals(18.0, lastV);
    }
}
