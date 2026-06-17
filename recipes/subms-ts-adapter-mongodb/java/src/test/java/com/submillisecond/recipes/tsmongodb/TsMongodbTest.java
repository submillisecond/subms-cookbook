package com.submillisecond.recipes.tsmongodb;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsNumericKind;
import com.submillisecond.recipes.ts.TsSchema;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import com.submillisecond.recipes.tsmongodb.BsonMapping.PointTuple;
import com.submillisecond.recipes.tsmongodb.BsonMapping.SeriesDocs;
import java.util.List;
import org.bson.Document;
import org.junit.jupiter.api.Test;

class TsMongodbTest {

    private static TsSeriesMetadata tagged(long id, String name) {
        return new TsSeriesMetadata(id, name)
                .withSchema(TsSchema.numeric("ms", TsNumericKind.GAUGE))
                .withTag("host", "edge-01")
                .withTag("region", "us-east-1");
    }

    private static TsSeriesD series(long id, String name, int n) {
        TsSeriesD s = new TsSeriesD();
        long base = 1_780_000_000_000_000_000L;
        for (int i = 0; i < n; i++) {
            s.push(base + (long) i * 1_000_000_000L, i * 0.5);
        }
        return s.withMetadata(tagged(id, name));
    }

    private static String hex(byte[] b) {
        StringBuilder sb = new StringBuilder(b.length * 2);
        for (byte x : b) {
            sb.append(String.format("%02x", x));
        }
        return sb.toString();
    }

    @Test
    void pointDocHasCanonicalShape() {
        Document d = BsonMapping.pointDoc(7, 1_780_000_000_000_000_000L, 0.42);
        Document id = d.get("_id", Document.class);
        assertEquals(7L, id.getLong("sid"));
        assertEquals(1_780_000_000_000_000_000L, id.getLong("ts"));
        assertEquals(0.42, d.getDouble("v"));
    }

    @Test
    void pointCollectionName() {
        assertEquals("ts_42", BsonMapping.pointCollection(42));
    }

    @Test
    void pointRoundTripsThroughDoc() {
        PointTuple t = BsonMapping.pointFromDoc(BsonMapping.pointDoc(1, 100, 3.5));
        assertEquals(100, t.ts());
        assertEquals(3.5, t.value());
    }

    @Test
    void pointRoundTripsThroughBsonBytes() {
        Document d = BsonMapping.pointDoc(1, 100, 3.5);
        Document back = BsonMapping.docFromBytes(BsonMapping.docToBytes(d));
        assertEquals(d, back);
    }

    // Pins the exact BSON byte layout - identical to the Rust port's fixture, so
    // a Rust-encoded point document decodes here unchanged and vice versa.
    @Test
    void pointBsonMatchesCrossLanguageFixture() {
        byte[] bytes = BsonMapping.docToBytes(BsonMapping.pointDoc(1, 100, 3.5));
        assertEquals(
                "33000000035f6964001e00000012736964000100000000000000127473006400000000000000000176000000000000000c4000",
                hex(bytes));
    }

    @Test
    void metaDocPreservesIdentity() {
        TsSeriesMetadata back = BsonMapping.metaFromDoc(BsonMapping.metaDoc(tagged(9, "trades.aapl")));
        assertEquals(9, back.id());
        assertEquals("trades.aapl", back.name());
        assertEquals("edge-01", back.tags().get("host"));
        assertEquals("us-east-1", back.tags().get("region"));
        assertTrue(back.schema() instanceof TsSchema.Numeric);
        TsSchema.Numeric num = (TsSchema.Numeric) back.schema();
        assertEquals("ms", num.unit().orElse(""));
        assertEquals(TsNumericKind.GAUGE, num.kind());
    }

    @Test
    void metaDocAnonymousSchemaRoundTrips() {
        TsSeriesMetadata back =
                BsonMapping.metaFromDoc(BsonMapping.metaDoc(new TsSeriesMetadata(1, "x")));
        assertTrue(back.schema() instanceof TsSchema.Anonymous);
    }

    @Test
    void seriesRoundTripsThroughDocs() {
        TsSeriesD s = series(3, "cpu", 16);
        SeriesDocs sd = BsonMapping.seriesToDocs(s);
        assertEquals(16, sd.points().size());
        TsSeriesD back = BsonMapping.seriesFromDocs(sd.meta(), sd.points());
        assertEquals(16, back.size());
        assertEquals("cpu", back.metadata().orElseThrow().name());
        assertEquals(s.last().orElseThrow().value(), back.last().orElseThrow().value());
    }

    @Test
    void seriesFromDocsSortsUnorderedPoints() {
        Document meta = BsonMapping.metaDoc(tagged(1, "cpu"));
        List<Document> points = List.of(
                BsonMapping.pointDoc(1, 300, 3.0),
                BsonMapping.pointDoc(1, 100, 1.0),
                BsonMapping.pointDoc(1, 200, 2.0));
        TsSeriesD s = BsonMapping.seriesFromDocs(meta, points);
        assertEquals(List.of(100L, 200L, 300L), s.toList().stream().map(p -> p.ts()).toList());
    }

    @Test
    void seriesWithNoMetadataMapsToIdZero() {
        TsSeriesD s = new TsSeriesD();
        s.push(10, 1.0);
        SeriesDocs sd = BsonMapping.seriesToDocs(s);
        assertEquals(0L, sd.meta().getLong("_id"));
    }

    @Test
    void pointFromDocRejectsMalformed() {
        Document bad = new Document("_id", 5L).append("v", 1.0);
        TsMongoException ex = assertThrows(TsMongoException.class, () -> BsonMapping.pointFromDoc(bad));
        assertEquals(TsMongoException.Kind.MAPPING, ex.kind());
    }

    @Test
    void adapterWriteThenReadSeries() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        assertEquals(32, a.writeSeries(series(5, "cpu", 32)));
        TsSeriesD back = a.readSeries(5);
        assertEquals(32, back.size());
        assertEquals("cpu", back.metadata().orElseThrow().name());
    }

    @Test
    void adapterReadUnknownSeriesThrows() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        TsMongoException ex = assertThrows(TsMongoException.class, () -> a.readSeries(99));
        assertEquals(TsMongoException.Kind.MAPPING, ex.kind());
    }

    @Test
    void adapterWriteThenReadCollection() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        a.writeSeries(series(1, "a", 8));
        a.writeSeries(series(2, "b", 8));
        a.writeSeries(series(3, "c", 8));
        TsCollection<Double> back = a.readCollection();
        assertEquals(3, back.size());
        assertEquals(8, back.byName("b").orElseThrow().size());
    }

    @Test
    void adapterEmptySeriesWritesMetaOnly() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        TsSeriesD empty = new TsSeriesD().withMetadata(tagged(4, "empty"));
        assertEquals(0, a.writeSeries(empty));
        assertEquals(1, a.store().count(BsonMapping.META_COLLECTION));
    }

    @Test
    void ensureIndexesCreatesCompoundIndex() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        a.writeSeries(series(5, "cpu", 4));
        a.ensureIndexes();
        List<Document> idx = a.store().indexes(BsonMapping.pointCollection(5));
        assertEquals(1, idx.size());
        assertEquals(new Document("_id.sid", 1).append("_id.ts", 1), idx.get(0));
        assertTrue(a.store().indexes(BsonMapping.META_COLLECTION).isEmpty());
    }

    @Test
    void changeEventsCaptureEveryInsert() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        a.writeSeries(series(5, "cpu", 4));
        List<TsChangeEvent> changes = a.pollChanges();
        assertEquals(5, changes.size()); // 1 meta + 4 points
        assertEquals(BsonMapping.META_COLLECTION, changes.get(0).collection());
        assertTrue(a.pollChanges().isEmpty());
    }

    @Test
    void storeFindOneById() {
        InMemoryMongoStore store = new InMemoryMongoStore();
        store.insertMany("ts_1", List.of(BsonMapping.pointDoc(1, 100, 1.0), BsonMapping.pointDoc(1, 200, 2.0)));
        Document id = new Document("sid", 1L).append("ts", 200L);
        assertEquals(2.0, store.findOne("ts_1", id).orElseThrow().getDouble("v"));
        Document missing = new Document("sid", 1L).append("ts", 999L);
        assertFalse(store.findOne("ts_1", missing).isPresent());
    }

    @Test
    void storeCollectionsListsWrittenNames() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        a.writeSeries(series(1, "a", 2));
        a.writeSeries(series(2, "b", 2));
        List<String> names = a.store().collections();
        assertEquals(List.of("ts_1", "ts_2", "ts_meta"), names);
    }

    @Test
    void writeCollectionMapsGenericSeries() {
        TsMongoAdapter<InMemoryMongoStore> a = new TsMongoAdapter<>(new InMemoryMongoStore());
        TsCollection<Double> coll = new TsCollection<>();
        for (long id = 1; id <= 3; id++) {
            coll.register(tagged(id, "g" + id));
            for (int i = 0; i < 5; i++) {
                coll.push(id, 1_000L + i, i * 1.0);
            }
        }
        assertEquals(15, a.writeCollection(coll));
        assertEquals(3, a.readCollection().size());
    }

    @Test
    void counterAndRateKindsRoundTrip() {
        for (TsNumericKind kind : new TsNumericKind[] {TsNumericKind.COUNTER, TsNumericKind.RATE}) {
            TsSeriesMetadata m = new TsSeriesMetadata(1, "k").withSchema(TsSchema.numeric(null, kind));
            TsSeriesMetadata back = BsonMapping.metaFromDoc(BsonMapping.metaDoc(m));
            assertEquals(kind, ((TsSchema.Numeric) back.schema()).kind());
        }
    }

    @Test
    void customAndSchemalessSchemaRoundTrip() {
        TsSeriesMetadata custom =
                new TsSeriesMetadata(1, "c").withSchema(new TsSchema.Custom("Trade"));
        assertTrue(BsonMapping.metaFromDoc(BsonMapping.metaDoc(custom)).schema() instanceof TsSchema.Custom);

        TsSeriesMetadata schemaless =
                new TsSeriesMetadata(2, "s").withSchema(new TsSchema.Schemaless());
        assertTrue(
                BsonMapping.metaFromDoc(BsonMapping.metaDoc(schemaless)).schema()
                        instanceof TsSchema.Schemaless);
    }

    @Test
    void metaFromDocRejectsMissingId() {
        Document bad = new Document("name", "x");
        TsMongoException ex = assertThrows(TsMongoException.class, () -> BsonMapping.metaFromDoc(bad));
        assertEquals(TsMongoException.Kind.MAPPING, ex.kind());
    }

    @Test
    void docFromBytesRejectsGarbage() {
        TsMongoException ex =
                assertThrows(TsMongoException.class, () -> BsonMapping.docFromBytes(new byte[] {1, 2, 3, 4}));
        assertEquals(TsMongoException.Kind.BSON, ex.kind());
    }

    @Test
    void exceptionFactoriesCarryKind() {
        assertEquals(TsMongoException.Kind.CONFIG, TsMongoException.config("c").kind());
        assertEquals(TsMongoException.Kind.STORE, TsMongoException.store("s").kind());
        assertEquals(TsMongoException.Kind.BSON, TsMongoException.bson("b").kind());
        assertEquals(TsMongoException.Kind.MAPPING, TsMongoException.mapping("m").kind());
    }

    @Test
    void storeDefaultDrainChangesIsEmpty() {
        TsMongoStore minimal = new TsMongoStore() {
            @Override
            public long insertMany(String c, List<Document> d) {
                return 0;
            }

            @Override
            public List<Document> findAll(String c) {
                return List.of();
            }

            @Override
            public java.util.Optional<Document> findOne(String c, Object id) {
                return java.util.Optional.empty();
            }

            @Override
            public void createIndex(String c, Document keys) {}

            @Override
            public List<String> collections() {
                return List.of();
            }
        };
        assertTrue(minimal.drainChanges().isEmpty());
    }
}
