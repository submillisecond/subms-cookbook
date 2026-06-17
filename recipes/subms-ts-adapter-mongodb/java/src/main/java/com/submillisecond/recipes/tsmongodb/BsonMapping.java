package com.submillisecond.recipes.tsmongodb;

import com.submillisecond.recipes.ts.TsNumericKind;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSchema;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.nio.ByteBuffer;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import org.bson.BsonBinaryReader;
import org.bson.BsonBinaryWriter;
import org.bson.Document;
import org.bson.codecs.DecoderContext;
import org.bson.codecs.DocumentCodec;
import org.bson.codecs.EncoderContext;
import org.bson.io.BasicOutputBuffer;

/**
 * The pure mapping core: {@link TsSeriesD} / metadata &lt;-&gt; BSON documents.
 *
 * <p>Point documents follow the canonical MongoDB time-series shape
 * {@code { _id: { sid, ts }, v }}; series identity lives in a sidecar metadata
 * document keyed by the numeric series id. The byte layout is BSON-spec
 * canonical, so a Rust-encoded point document decodes here unchanged - pinned
 * by a cross-language hex fixture in both test suites.
 */
public final class BsonMapping {

    private BsonMapping() {}

    /** The collection that holds one metadata document per series. */
    public static final String META_COLLECTION = "ts_meta";

    private static final DocumentCodec CODEC = new DocumentCodec();

    /** The per-series point collection name ({@code ts_<sid>}). */
    public static String pointCollection(long seriesId) {
        return "ts_" + seriesId;
    }

    /** Build the per-point document {@code { _id: { sid, ts }, v }}. */
    public static Document pointDoc(long seriesId, long ts, double value) {
        Document id = new Document("sid", seriesId).append("ts", ts);
        return new Document("_id", id).append("v", value);
    }

    /** Build the sidecar metadata document for a series' metadata. */
    public static Document metaDoc(TsSeriesMetadata meta) {
        Document tags = new Document();
        for (Map.Entry<String, String> e : meta.tags().entrySet()) {
            tags.append(e.getKey(), e.getValue());
        }
        Document d = new Document("_id", meta.id()).append("name", meta.name()).append("tags", tags);
        Document schema = schemaDoc(meta.schema());
        if (schema != null) {
            d.append("schema", schema);
        }
        return d;
    }

    private static Document schemaDoc(TsSchema schema) {
        if (schema instanceof TsSchema.Numeric n) {
            Document s = new Document("kind", numericKindStr(n.kind()));
            n.unit().ifPresent(u -> s.append("unit", u));
            return s;
        } else if (schema instanceof TsSchema.Custom c) {
            return new Document("kind", "custom").append("type_name", c.typeName());
        } else if (schema instanceof TsSchema.Schemaless) {
            return new Document("kind", "schemaless");
        }
        return null;
    }

    private static String numericKindStr(TsNumericKind kind) {
        return switch (kind) {
            case GAUGE -> "gauge";
            case COUNTER -> "counter";
            case RATE -> "rate";
        };
    }

    private static TsSchema parseSchema(Document d) {
        if (d == null) {
            return TsSchema.anonymous();
        }
        String kind = d.getString("kind");
        if (kind == null) {
            return TsSchema.anonymous();
        }
        return switch (kind) {
            case "gauge", "counter", "rate" -> {
                TsNumericKind k =
                        switch (kind) {
                            case "counter" -> TsNumericKind.COUNTER;
                            case "rate" -> TsNumericKind.RATE;
                            default -> TsNumericKind.GAUGE;
                        };
                yield TsSchema.numeric(d.getString("unit"), k);
            }
            case "schemaless" -> new TsSchema.Schemaless();
            case "custom" -> new TsSchema.Custom(d.getString("type_name") == null ? "" : d.getString("type_name"));
            default -> TsSchema.anonymous();
        };
    }

    /** Reconstruct metadata from a sidecar document. */
    public static TsSeriesMetadata metaFromDoc(Document d) {
        Long id = asLong(d.get("_id"));
        if (id == null) {
            throw TsMongoException.mapping("meta doc missing i64 _id");
        }
        String name = d.getString("name");
        TsSeriesMetadata meta =
                new TsSeriesMetadata(id, name == null ? "" : name).withSchema(parseSchema(asDoc(d.get("schema"))));
        Document tags = asDoc(d.get("tags"));
        if (tags != null) {
            for (Map.Entry<String, Object> e : tags.entrySet()) {
                if (e.getValue() instanceof String v) {
                    meta = meta.withTag(e.getKey(), v);
                }
            }
        }
        return meta;
    }

    /** A decoded (ts, value) pair. */
    public record PointTuple(long ts, double value) {}

    /** Decode one point document into its (ts, value). */
    public static PointTuple pointFromDoc(Document d) {
        Document id = asDoc(d.get("_id"));
        if (id == null) {
            throw TsMongoException.mapping("point doc missing _id sub-document");
        }
        Long ts = asLong(id.get("ts"));
        if (ts == null) {
            throw TsMongoException.mapping("point _id missing i64 ts");
        }
        Object v = d.get("v");
        if (!(v instanceof Number n)) {
            throw TsMongoException.mapping("point doc missing f64 v");
        }
        return new PointTuple(ts, n.doubleValue());
    }

    /** A series mapped to its (metadata document, point documents) pair. */
    public record SeriesDocs(Document meta, List<Document> points) {}

    /** Turn the unboxed-double fast-path series into its documents. */
    public static SeriesDocs seriesToDocs(TsSeriesD series) {
        return toDocs(series.metadata().orElse(null), series.toList());
    }

    /** Turn a generic double series (a collection element) into its documents. */
    public static SeriesDocs seriesToDocs(
            com.submillisecond.recipes.ts.TsSeries<Double> series) {
        List<TsPoint<Double>> points = new ArrayList<>();
        series.forEach(points::add);
        return toDocs(series.metadata().orElse(null), points);
    }

    private static SeriesDocs toDocs(TsSeriesMetadata metaOrNull, List<TsPoint<Double>> pts) {
        TsSeriesMetadata meta = metaOrNull == null ? new TsSeriesMetadata(0, "") : metaOrNull;
        long sid = meta.id();
        List<Document> points = new ArrayList<>(pts.size());
        for (TsPoint<Double> p : pts) {
            points.add(pointDoc(sid, p.ts(), p.value()));
        }
        return new SeriesDocs(metaDoc(meta), points);
    }

    /**
     * Rebuild a series from its metadata document and an unordered list of point
     * documents. Points are sorted by ts before they are pushed, because
     * {@code TsSeriesD.push} enforces a monotonic time axis.
     */
    public static TsSeriesD seriesFromDocs(Document meta, List<Document> points) {
        TsSeriesMetadata m = metaFromDoc(meta);
        List<PointTuple> decoded = new ArrayList<>(points.size());
        for (Document p : points) {
            decoded.add(pointFromDoc(p));
        }
        decoded.sort((a, b) -> Long.compare(a.ts(), b.ts()));
        TsSeriesD series = new TsSeriesD();
        series.setMetadata(m);
        for (PointTuple p : decoded) {
            series.push(p.ts(), p.value());
        }
        return series;
    }

    /** Encode a document to its canonical BSON byte form. */
    public static byte[] docToBytes(Document d) {
        try (BasicOutputBuffer buf = new BasicOutputBuffer()) {
            try (BsonBinaryWriter w = new BsonBinaryWriter(buf)) {
                CODEC.encode(w, d, EncoderContext.builder().build());
            }
            return buf.toByteArray();
        } catch (RuntimeException e) {
            throw TsMongoException.bson(e.getMessage());
        }
    }

    /** Decode BSON bytes back into a document. */
    public static Document docFromBytes(byte[] bytes) {
        try (BsonBinaryReader r = new BsonBinaryReader(ByteBuffer.wrap(bytes))) {
            return CODEC.decode(r, DecoderContext.builder().build());
        } catch (RuntimeException e) {
            throw TsMongoException.bson(e.getMessage());
        }
    }

    private static Long asLong(Object o) {
        return (o instanceof Number n) ? n.longValue() : null;
    }

    private static Document asDoc(Object o) {
        return (o instanceof Document d) ? d : null;
    }
}
