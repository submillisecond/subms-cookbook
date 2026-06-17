package com.submillisecond.recipes.tsmongodb;

import static com.submillisecond.recipes.tsmongodb.BsonMapping.META_COLLECTION;
import static com.submillisecond.recipes.tsmongodb.BsonMapping.pointCollection;
import static com.submillisecond.recipes.tsmongodb.BsonMapping.seriesFromDocs;
import static com.submillisecond.recipes.tsmongodb.BsonMapping.seriesToDocs;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import com.submillisecond.recipes.tsmongodb.BsonMapping.SeriesDocs;
import java.util.List;
import org.bson.Document;

/** A MongoDB adapter parameterised over a store. */
public final class TsMongoAdapter<S extends TsMongoStore> {

    private final S store;

    public TsMongoAdapter(S store) {
        this.store = store;
    }

    /** Borrow the underlying store (the inspection point under test). */
    public S store() {
        return store;
    }

    /** Write one fast-path series: its metadata document plus one per point. */
    public long writeSeries(TsSeriesD series) {
        SeriesDocs sd = seriesToDocs(series);
        long sid = series.metadata().map(TsSeriesMetadata::id).orElse(0L);
        return writeDocs(sd, sid);
    }

    /** Write one generic double series (a collection element). */
    public long writeSeries(TsSeries<Double> series) {
        SeriesDocs sd = seriesToDocs(series);
        long sid = series.metadata().map(TsSeriesMetadata::id).orElse(0L);
        return writeDocs(sd, sid);
    }

    private long writeDocs(SeriesDocs sd, long sid) {
        store.insertMany(META_COLLECTION, List.of(sd.meta()));
        if (sd.points().isEmpty()) {
            return 0;
        }
        return store.insertMany(pointCollection(sid), sd.points());
    }

    /** Write every series in a collection. Returns the total point count. */
    public long writeCollection(TsCollection<Double> coll) {
        long total = 0;
        for (TsSeries<Double> s : coll.series()) {
            total += writeSeries(s);
        }
        return total;
    }

    /** Read one series back by its numeric id. */
    public TsSeriesD readSeries(long seriesId) {
        Document meta =
                store.findOne(META_COLLECTION, seriesId)
                        .orElseThrow(
                                () ->
                                        TsMongoException.mapping(
                                                "no metadata for series " + seriesId));
        List<Document> points = store.findAll(pointCollection(seriesId));
        return seriesFromDocs(meta, points);
    }

    /** Read every stored series into a collection. */
    public TsCollection<Double> readCollection() {
        TsCollection<Double> out = new TsCollection<>();
        for (Document metaDoc : store.findAll(META_COLLECTION)) {
            TsSeriesMetadata m = BsonMapping.metaFromDoc(metaDoc);
            long sid = m.id();
            TsSeriesD series = seriesFromDocs(metaDoc, store.findAll(pointCollection(sid)));
            out.register(m);
            for (TsPoint<Double> p : series.toList()) {
                out.push(sid, p.ts(), p.value());
            }
        }
        return out;
    }

    /**
     * Ensure the {@code (_id.sid, _id.ts)} compound index exists on every point
     * collection currently present.
     */
    public void ensureIndexes() {
        for (String name : store.collections()) {
            if (name.startsWith("ts_") && !name.equals(META_COLLECTION)) {
                store.createIndex(name, new Document("_id.sid", 1).append("_id.ts", 1));
            }
        }
    }

    /** Drain captured change events (the change-data-capture surface). */
    public List<TsChangeEvent> pollChanges() {
        return store.drainChanges();
    }
}
