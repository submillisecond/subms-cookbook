package com.submillisecond.recipes.tsmongodb;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;
import java.util.Optional;
import java.util.TreeMap;
import org.bson.Document;

/**
 * A hermetic in-memory document store. Holds collections as ordered lists,
 * records created indexes, and logs every insert as a change event.
 */
public final class InMemoryMongoStore implements TsMongoStore {

    private final TreeMap<String, List<Document>> collections = new TreeMap<>();
    private final TreeMap<String, List<Document>> indexes = new TreeMap<>();
    private final List<TsChangeEvent> changes = new ArrayList<>();

    @Override
    public synchronized long insertMany(String collection, List<Document> docs) {
        List<Document> bucket = collections.computeIfAbsent(collection, k -> new ArrayList<>());
        for (Document d : docs) {
            Document copy = new Document(d);
            bucket.add(copy);
            changes.add(TsChangeEvent.insert(collection, copy));
        }
        return docs.size();
    }

    @Override
    public synchronized List<Document> findAll(String collection) {
        List<Document> bucket = collections.get(collection);
        return bucket == null ? new ArrayList<>() : new ArrayList<>(bucket);
    }

    @Override
    public synchronized Optional<Document> findOne(String collection, Object id) {
        List<Document> bucket = collections.get(collection);
        if (bucket == null) {
            return Optional.empty();
        }
        return bucket.stream().filter(d -> Objects.equals(d.get("_id"), id)).findFirst();
    }

    @Override
    public synchronized void createIndex(String collection, Document keys) {
        indexes.computeIfAbsent(collection, k -> new ArrayList<>()).add(keys);
    }

    @Override
    public synchronized List<String> collections() {
        return new ArrayList<>(collections.keySet());
    }

    @Override
    public synchronized List<TsChangeEvent> drainChanges() {
        List<TsChangeEvent> out = new ArrayList<>(changes);
        changes.clear();
        return out;
    }

    /** Number of documents currently held in a collection. */
    public synchronized int count(String collection) {
        List<Document> bucket = collections.get(collection);
        return bucket == null ? 0 : bucket.size();
    }

    /** Index key documents recorded for a collection. */
    public synchronized List<Document> indexes(String collection) {
        List<Document> idx = indexes.get(collection);
        return idx == null ? new ArrayList<>() : new ArrayList<>(idx);
    }
}
