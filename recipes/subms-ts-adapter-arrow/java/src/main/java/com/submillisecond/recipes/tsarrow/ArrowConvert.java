package com.submillisecond.recipes.tsarrow;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.nio.channels.Channels;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import org.apache.arrow.memory.BufferAllocator;
import org.apache.arrow.vector.BigIntVector;
import org.apache.arrow.vector.FieldVector;
import org.apache.arrow.vector.Float8Vector;
import org.apache.arrow.vector.VectorSchemaRoot;
import org.apache.arrow.vector.ipc.ArrowStreamReader;
import org.apache.arrow.vector.ipc.ArrowStreamWriter;
import org.apache.arrow.vector.types.FloatingPointPrecision;
import org.apache.arrow.vector.types.pojo.ArrowType;
import org.apache.arrow.vector.types.pojo.Field;
import org.apache.arrow.vector.types.pojo.FieldType;
import org.apache.arrow.vector.types.pojo.Schema;

/**
 * The pure mapping core: {@link TsSeriesD} / {@link TsCollection} &lt;-&gt; Arrow
 * {@link VectorSchemaRoot} and Arrow IPC streams.
 *
 * <p>A single series maps to a two-column root ({@code ts: Int64},
 * {@code v: Float64}) with its identity in schema metadata; a collection maps to
 * the tidy long-format root ({@code sid}, {@code ts}, {@code v}). Both round-trip
 * through the Arrow IPC stream format, so the data hands off to Polars / DuckDB /
 * pandas with no translation layer. The IPC bytes are Arrow-spec-compliant and
 * cross-readable by the Rust port; byte-identical IPC across independent Arrow
 * implementations is not claimed.
 */
public final class ArrowConvert {

    private ArrowConvert() {}

    private static final String MK_SID = "subms.sid";
    private static final String MK_NAME = "subms.name";
    private static final String TAG_PREFIX = "subms.tag.";
    private static final String NAME_PREFIX = "subms.name.";

    private static Field i64(String name) {
        return new Field(name, FieldType.notNullable(new ArrowType.Int(64, true)), null);
    }

    private static Field f64(String name) {
        return new Field(
                name,
                FieldType.notNullable(new ArrowType.FloatingPoint(FloatingPointPrecision.DOUBLE)),
                null);
    }

    private static Map<String, String> metaToMap(TsSeriesMetadata meta) {
        Map<String, String> md = new HashMap<>();
        md.put(MK_SID, Long.toString(meta.id()));
        md.put(MK_NAME, meta.name());
        for (Map.Entry<String, String> e : meta.tags().entrySet()) {
            md.put(TAG_PREFIX + e.getKey(), e.getValue());
        }
        return md;
    }

    private static TsSeriesMetadata mapToMeta(Map<String, String> md) {
        String sidStr = md.get(MK_SID);
        if (sidStr == null) {
            return null;
        }
        long sid;
        try {
            sid = Long.parseLong(sidStr);
        } catch (NumberFormatException e) {
            return null;
        }
        TsSeriesMetadata meta = new TsSeriesMetadata(sid, md.getOrDefault(MK_NAME, ""));
        for (Map.Entry<String, String> e : md.entrySet()) {
            if (e.getKey().startsWith(TAG_PREFIX)) {
                meta = meta.withTag(e.getKey().substring(TAG_PREFIX.length()), e.getValue());
            }
        }
        return meta;
    }

    /** Map one series to a two-column root. The caller owns and closes the root. */
    public static VectorSchemaRoot seriesToRoot(TsSeriesD series, BufferAllocator alloc) {
        Map<String, String> md =
                series.metadata().map(ArrowConvert::metaToMap).orElseGet(HashMap::new);
        Schema schema = new Schema(List.of(i64("ts"), f64("v")), md);
        VectorSchemaRoot root = VectorSchemaRoot.create(schema, alloc);
        BigIntVector ts = (BigIntVector) root.getVector("ts");
        Float8Vector v = (Float8Vector) root.getVector("v");
        List<TsPoint<Double>> pts = series.toList();
        int n = pts.size();
        ts.allocateNew(n);
        v.allocateNew(n);
        for (int i = 0; i < n; i++) {
            ts.set(i, pts.get(i).ts());
            v.set(i, pts.get(i).value());
        }
        ts.setValueCount(n);
        v.setValueCount(n);
        root.setRowCount(n);
        return root;
    }

    /**
     * Refill an already-allocated two-column root from a series, in place. This
     * is the steady-state streaming pattern: allocate one root, then refill it
     * per batch, so the hot path does no off-heap allocation. The root must have
     * the {@code ts}/{@code v} columns (e.g. from a prior {@link #seriesToRoot}).
     */
    public static void fillSeriesRoot(VectorSchemaRoot root, TsSeriesD series) {
        BigIntVector ts = reqI64(root, "ts");
        Float8Vector v = reqF64(root, "v");
        List<TsPoint<Double>> pts = series.toList();
        int n = pts.size();
        for (int i = 0; i < n; i++) {
            ts.set(i, pts.get(i).ts());
            v.set(i, pts.get(i).value());
        }
        ts.setValueCount(n);
        v.setValueCount(n);
        root.setRowCount(n);
    }

    private static BigIntVector reqI64(VectorSchemaRoot root, String name) {
        FieldVector fv = root.getVector(name);
        if (fv == null) {
            throw TsArrowException.mapping("batch missing column " + name);
        }
        if (!(fv instanceof BigIntVector b)) {
            throw TsArrowException.mapping("column " + name + " is not Int64");
        }
        return b;
    }

    private static Float8Vector reqF64(VectorSchemaRoot root, String name) {
        FieldVector fv = root.getVector(name);
        if (fv == null) {
            throw TsArrowException.mapping("batch missing column " + name);
        }
        if (!(fv instanceof Float8Vector f)) {
            throw TsArrowException.mapping("column " + name + " is not Float64");
        }
        return f;
    }

    /**
     * Rebuild a series from a two-column root. A time-ordered batch (the common
     * case - it is how the arc emits them) takes an allocation-light fast path;
     * an out-of-order batch falls back to an index sort.
     */
    public static TsSeriesD rootToSeries(VectorSchemaRoot root) {
        BigIntVector ts = reqI64(root, "ts");
        Float8Vector v = reqF64(root, "v");
        int n = root.getRowCount();
        TsSeriesD series = TsSeriesD.withCapacity(n);
        TsSeriesMetadata meta = mapToMeta(root.getSchema().getCustomMetadata());
        if (meta != null) {
            series.setMetadata(meta);
        }
        boolean sorted = true;
        for (int i = 1; i < n; i++) {
            if (ts.get(i) < ts.get(i - 1)) {
                sorted = false;
                break;
            }
        }
        if (sorted) {
            for (int i = 0; i < n; i++) {
                series.push(ts.get(i), v.get(i));
            }
        } else {
            Integer[] idx = new Integer[n];
            for (int i = 0; i < n; i++) {
                idx[i] = i;
            }
            java.util.Arrays.sort(idx, (a, b) -> Long.compare(ts.get(a), ts.get(b)));
            for (int i : idx) {
                series.push(ts.get(i), v.get(i));
            }
        }
        return series;
    }

    /** Map a collection to the long-format root ({@code sid}, {@code ts}, {@code v}). */
    public static VectorSchemaRoot collectionToRoot(TsCollection<Double> coll, BufferAllocator alloc) {
        Map<String, String> md = new HashMap<>();
        List<long[]> rows = new ArrayList<>();
        for (TsSeries<Double> s : coll.series()) {
            long sid = s.metadata().map(TsSeriesMetadata::id).orElse(0L);
            s.metadata().ifPresent(m -> md.put(NAME_PREFIX + m.id(), m.name()));
            for (TsPoint<Double> p : s) {
                rows.add(new long[] {sid, p.ts(), Double.doubleToLongBits(p.value())});
            }
        }
        Schema schema = new Schema(List.of(i64("sid"), i64("ts"), f64("v")), md);
        VectorSchemaRoot root = VectorSchemaRoot.create(schema, alloc);
        BigIntVector sidVec = (BigIntVector) root.getVector("sid");
        BigIntVector tsVec = (BigIntVector) root.getVector("ts");
        Float8Vector vVec = (Float8Vector) root.getVector("v");
        int n = rows.size();
        sidVec.allocateNew(n);
        tsVec.allocateNew(n);
        vVec.allocateNew(n);
        for (int i = 0; i < n; i++) {
            long[] r = rows.get(i);
            sidVec.set(i, r[0]);
            tsVec.set(i, r[1]);
            vVec.set(i, Double.longBitsToDouble(r[2]));
        }
        sidVec.setValueCount(n);
        tsVec.setValueCount(n);
        vVec.setValueCount(n);
        root.setRowCount(n);
        return root;
    }

    /** Rebuild a collection from a long-format root. */
    public static TsCollection<Double> rootToCollection(VectorSchemaRoot root) {
        BigIntVector sid = reqI64(root, "sid");
        BigIntVector ts = reqI64(root, "ts");
        Float8Vector v = reqF64(root, "v");
        Map<String, String> md = root.getSchema().getCustomMetadata();
        int n = root.getRowCount();
        long[][] rows = new long[n][];
        for (int i = 0; i < n; i++) {
            rows[i] = new long[] {sid.get(i), ts.get(i), Double.doubleToLongBits(v.get(i))};
        }
        java.util.Arrays.sort(rows, (a, b) -> a[0] != b[0] ? Long.compare(a[0], b[0]) : Long.compare(a[1], b[1]));
        TsCollection<Double> coll = new TsCollection<>();
        long current = Long.MIN_VALUE;
        boolean any = false;
        for (long[] r : rows) {
            if (!any || r[0] != current) {
                coll.register(new TsSeriesMetadata(r[0], md.getOrDefault(NAME_PREFIX + r[0], "")));
                current = r[0];
                any = true;
            }
            coll.push(r[0], r[1], Double.longBitsToDouble(r[2]));
        }
        return coll;
    }

    /** Serialise a root to an Arrow IPC stream. */
    public static byte[] writeIpc(VectorSchemaRoot root) {
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        try (ArrowStreamWriter w = new ArrowStreamWriter(root, null, Channels.newChannel(out))) {
            w.start();
            w.writeBatch();
            w.end();
        } catch (Exception e) {
            throw TsArrowException.ipc(String.valueOf(e.getMessage()));
        }
        return out.toByteArray();
    }

    /** Build, serialise, and free a series in one call. */
    public static byte[] seriesToIpc(TsSeriesD series, BufferAllocator alloc) {
        try (VectorSchemaRoot root = seriesToRoot(series, alloc)) {
            return writeIpc(root);
        }
    }

    /** Read the first batch of an IPC stream back into a series. */
    public static TsSeriesD ipcToSeries(byte[] bytes, BufferAllocator alloc) {
        try (ArrowStreamReader reader =
                new ArrowStreamReader(new ByteArrayInputStream(bytes), alloc)) {
            if (!reader.loadNextBatch()) {
                throw TsArrowException.ipc("ipc stream held no record batch");
            }
            return rootToSeries(reader.getVectorSchemaRoot());
        } catch (TsArrowException e) {
            throw e;
        } catch (Exception e) {
            throw TsArrowException.ipc(String.valueOf(e.getMessage()));
        }
    }

    /** Read the first batch of an IPC stream back into a collection. */
    public static TsCollection<Double> ipcToCollection(byte[] bytes, BufferAllocator alloc) {
        try (ArrowStreamReader reader =
                new ArrowStreamReader(new ByteArrayInputStream(bytes), alloc)) {
            if (!reader.loadNextBatch()) {
                throw TsArrowException.ipc("ipc stream held no record batch");
            }
            return rootToCollection(reader.getVectorSchemaRoot());
        } catch (TsArrowException e) {
            throw e;
        } catch (Exception e) {
            throw TsArrowException.ipc(String.valueOf(e.getMessage()));
        }
    }
}
