package com.submillisecond.recipes.tsarrow;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.util.Base64;
import java.util.List;
import org.apache.arrow.memory.BufferAllocator;
import org.apache.arrow.memory.RootAllocator;
import org.apache.arrow.vector.BigIntVector;
import org.apache.arrow.vector.Float8Vector;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.types.FloatingPointPrecision;
import org.apache.arrow.vector.types.pojo.ArrowType;
import org.apache.arrow.vector.types.pojo.Field;
import org.apache.arrow.vector.types.pojo.FieldType;
import org.apache.arrow.vector.types.pojo.Schema;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

class TsArrowTest {

    private BufferAllocator alloc;

    @BeforeEach
    void setUp() {
        alloc = new RootAllocator();
    }

    @AfterEach
    void tearDown() {
        alloc.close();
    }

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
    void seriesRoundTripsThroughRoot() {
        TsSeriesD s = series(3, "cpu", 16);
        try (VectorSchemaRoot root = ArrowConvert.seriesToRoot(s, alloc)) {
            assertEquals(16, root.getRowCount());
            assertEquals(2, root.getSchema().getFields().size());
            TsSeriesD back = ArrowConvert.rootToSeries(root);
            assertEquals(16, back.size());
            assertEquals("cpu", back.metadata().orElseThrow().name());
            assertEquals(s.last().orElseThrow().value(), back.last().orElseThrow().value());
        }
    }

    @Test
    void rootPreservesSchemaMetadata() {
        try (VectorSchemaRoot root = ArrowConvert.seriesToRoot(series(9, "trades.aapl", 4), alloc)) {
            var md = root.getSchema().getCustomMetadata();
            assertEquals("9", md.get("subms.sid"));
            assertEquals("trades.aapl", md.get("subms.name"));
            assertEquals("edge-01", md.get("subms.tag.host"));
        }
    }

    @Test
    void rootToSeriesSortsUnorderedRows() {
        Schema schema = new Schema(List.of(
                new Field("ts", FieldType.notNullable(new ArrowType.Int(64, true)), null),
                new Field("v", FieldType.notNullable(new ArrowType.FloatingPoint(FloatingPointPrecision.DOUBLE)), null)));
        try (VectorSchemaRoot root = VectorSchemaRoot.create(schema, alloc)) {
            BigIntVector ts = (BigIntVector) root.getVector("ts");
            Float8Vector v = (Float8Vector) root.getVector("v");
            ts.allocateNew(3);
            v.allocateNew(3);
            long[] tss = {300, 100, 200};
            double[] vs = {3.0, 1.0, 2.0};
            for (int i = 0; i < 3; i++) {
                ts.set(i, tss[i]);
                v.set(i, vs[i]);
            }
            ts.setValueCount(3);
            v.setValueCount(3);
            root.setRowCount(3);
            TsSeriesD s = ArrowConvert.rootToSeries(root);
            assertEquals(List.of(100L, 200L, 300L), s.toList().stream().map(p -> p.ts()).toList());
        }
    }

    @Test
    void seriesWithNoMetadataRoundTrips() {
        TsSeriesD s = new TsSeriesD();
        s.push(10, 1.0);
        s.push(20, 2.0);
        try (VectorSchemaRoot root = ArrowConvert.seriesToRoot(s, alloc)) {
            TsSeriesD back = ArrowConvert.rootToSeries(root);
            assertEquals(2, back.size());
            assertTrue(back.metadata().isEmpty());
        }
    }

    @Test
    void missingColumnIsMappingError() {
        Schema schema = new Schema(List.of(
                new Field("ts", FieldType.notNullable(new ArrowType.Int(64, true)), null)));
        try (VectorSchemaRoot root = VectorSchemaRoot.create(schema, alloc)) {
            root.setRowCount(0);
            TsArrowException ex = assertThrows(TsArrowException.class, () -> ArrowConvert.rootToSeries(root));
            assertEquals(TsArrowException.Kind.MAPPING, ex.kind());
        }
    }

    @Test
    void wrongColumnTypeIsMappingError() {
        Schema schema = new Schema(List.of(
                new Field("ts", FieldType.notNullable(new ArrowType.FloatingPoint(FloatingPointPrecision.DOUBLE)), null),
                new Field("v", FieldType.notNullable(new ArrowType.FloatingPoint(FloatingPointPrecision.DOUBLE)), null)));
        try (VectorSchemaRoot root = VectorSchemaRoot.create(schema, alloc)) {
            root.setRowCount(0);
            TsArrowException ex = assertThrows(TsArrowException.class, () -> ArrowConvert.rootToSeries(root));
            assertEquals(TsArrowException.Kind.MAPPING, ex.kind());
        }
    }

    @Test
    void collectionRoundTripsThroughLongRoot() {
        TsCollection<Double> coll = new TsCollection<>();
        for (long id = 1; id <= 3; id++) {
            coll.register(new TsSeriesMetadata(id, "s" + id));
            for (int i = 0; i < 5; i++) {
                coll.push(id, 1_000 + i, i * 1.0);
            }
        }
        try (VectorSchemaRoot root = ArrowConvert.collectionToRoot(coll, alloc)) {
            assertEquals(15, root.getRowCount());
            assertEquals(3, root.getSchema().getFields().size());
            TsCollection<Double> back = ArrowConvert.rootToCollection(root);
            assertEquals(3, back.size());
            assertEquals(5, back.byName("s2").orElseThrow().size());
        }
    }

    @Test
    void emptyCollectionYieldsEmptyRoot() {
        try (VectorSchemaRoot root = ArrowConvert.collectionToRoot(new TsCollection<>(), alloc)) {
            assertEquals(0, root.getRowCount());
            assertEquals(0, ArrowConvert.rootToCollection(root).size());
        }
    }

    @Test
    void ipcRoundTripsSeries() {
        byte[] ipc = ArrowConvert.seriesToIpc(series(5, "cpu", 32), alloc);
        TsSeriesD back = ArrowConvert.ipcToSeries(ipc, alloc);
        assertEquals(32, back.size());
        assertEquals("cpu", back.metadata().orElseThrow().name());
    }

    @Test
    void ipcRoundTripsCollection() {
        TsCollection<Double> coll = new TsCollection<>();
        coll.register(new TsSeriesMetadata(7, "x"));
        for (int i = 0; i < 10; i++) {
            coll.push(7, 1_000 + i, i * 1.0);
        }
        byte[] ipc;
        try (VectorSchemaRoot root = ArrowConvert.collectionToRoot(coll, alloc)) {
            ipc = ArrowConvert.writeIpc(root);
        }
        TsCollection<Double> back = ArrowConvert.ipcToCollection(ipc, alloc);
        assertEquals(10, back.byName("x").orElseThrow().size());
    }

    @Test
    void ipcRejectsGarbage() {
        TsArrowException ex =
                assertThrows(TsArrowException.class, () -> ArrowConvert.ipcToSeries(new byte[] {1, 2, 3, 4}, alloc));
        assertEquals(TsArrowException.Kind.IPC, ex.kind());
    }

    // Cross-language interop: this IPC stream was written by the Rust port
    // (subms-ts-adapter-arrow) for series id=1 "cpu" {host=edge-01} with points
    // (100,1.5),(200,2.5),(300,3.5). Reading it here proves a Rust-encoded Arrow
    // stream decodes unchanged in Java - the actual interop guarantee (byte
    // identity across the two Arrow implementations is NOT claimed; cross-read is).
    private static final String RUST_IPC_B64 =
            "/////3gBAAAQAAAAAAAKAA4ADAALAAQACgAAABQAAAAAAAABBAAKAAwAAAAIAAQACgAAAAgAAACQAAAAAwAAAFwAAAAsAAAABAAAALj///8IAAAADAAAAAMAAABjcHUACgAAAHN1Ym1zLm5hbWUAANz///8IAAAADAAAAAEAAAAxAAAACQAAAHN1Ym1zLnNpZAAAAAgADAAIAAQACAAAAAgAAAAQAAAABwAAAGVkZ2UtMDEADgAAAHN1Ym1zLnRhZy5ob3N0AAACAAAAWAAAABQAAAAQABYAEAAAAA8ABAAAAAgAEAAAABgAAAAcAAAAAAAAAxgAAAAAAAYACAAGAAYAAAAAAAIAAAAAAAEAAAB2AAAAEAAUABAAAAAPAAQAAAAIABAAAAAYAAAAIAAAAAAAAAIcAAAACAAMAAQACwAIAAAAQAAAAAAAAAEAAAAAAgAAAHRzAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA/////7gAAAAQAAAADAAaABgAFwAEAAgADAAAACAAAAAAAQAAAAAAAAAAAAAAAAADBAAKABQADAAIAAQACgAAADQAAAAMAAAAAwAAAAAAAAACAAAAAwAAAAAAAAAAAAAAAAAAAAMAAAAAAAAAAAAAAAAAAAAEAAAAAAAAAAAAAAABAAAAAAAAAEAAAAAAAAAAGAAAAAAAAACAAAAAAAAAAAEAAAAAAAAAwAAAAAAAAAAYAAAAAAAAAAAAAAAAAAAA/wAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAGQAAAAAAAAAyAAAAAAAAAAsAQAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAD/AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA+D8AAAAAAAAEQAAAAAAAAAxAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAP////8AAAAA";

    @Test
    void readsRustWrittenIpcFixture() {
        byte[] bytes = Base64.getDecoder().decode(RUST_IPC_B64);
        TsSeriesD s = ArrowConvert.ipcToSeries(bytes, alloc);
        assertEquals(3, s.size());
        assertEquals("cpu", s.metadata().orElseThrow().name());
        assertEquals("edge-01", s.metadata().orElseThrow().tags().get("host"));
        assertEquals(List.of(100L, 200L, 300L), s.toList().stream().map(p -> p.ts()).toList());
        assertEquals(3.5, s.last().orElseThrow().value());
    }

    @Test
    void exceptionFactoriesCarryKind() {
        assertEquals(TsArrowException.Kind.MAPPING, TsArrowException.mapping("m").kind());
        assertEquals(TsArrowException.Kind.ARROW, TsArrowException.arrow("a").kind());
        assertEquals(TsArrowException.Kind.IPC, TsArrowException.ipc("i").kind());
    }

    @Test
    void writeIpcProducesNonEmptyStream() {
        try (VectorSchemaRoot root = ArrowConvert.seriesToRoot(series(1, "cpu", 4), alloc)) {
            assertFalse(ArrowConvert.writeIpc(root).length == 0);
        }
    }

    @Test
    void fillSeriesRootRefillsInPlace() {
        try (VectorSchemaRoot root = ArrowConvert.seriesToRoot(series(1, "cpu", 8), alloc)) {
            ArrowConvert.fillSeriesRoot(root, series(1, "cpu", 3));
            assertEquals(3, root.getRowCount());
            TsSeriesD back = ArrowConvert.rootToSeries(root);
            assertEquals(3, back.size());
            assertEquals(1.0, back.last().orElseThrow().value());
        }
    }

    @Test
    void ipcToCollectionRejectsGarbage() {
        TsArrowException ex = assertThrows(
                TsArrowException.class, () -> ArrowConvert.ipcToCollection(new byte[] {9, 8, 7}, alloc));
        assertEquals(TsArrowException.Kind.IPC, ex.kind());
    }

    @Test
    void malformedSidMetadataYieldsNoMetadata() {
        Schema schema = new Schema(
                List.of(
                        new Field("ts", FieldType.notNullable(new ArrowType.Int(64, true)), null),
                        new Field("v", FieldType.notNullable(new ArrowType.FloatingPoint(FloatingPointPrecision.DOUBLE)), null)),
                java.util.Map.of("subms.sid", "not-a-number"));
        try (VectorSchemaRoot root = VectorSchemaRoot.create(schema, alloc)) {
            BigIntVector ts = (BigIntVector) root.getVector("ts");
            Float8Vector v = (Float8Vector) root.getVector("v");
            ts.allocateNew(1);
            v.allocateNew(1);
            ts.set(0, 10);
            v.set(0, 1.0);
            ts.setValueCount(1);
            v.setValueCount(1);
            root.setRowCount(1);
            TsSeriesD s = ArrowConvert.rootToSeries(root);
            assertTrue(s.metadata().isEmpty());
        }
    }
}
