package com.submillisecond.recipes.tsmongodb;

import com.mongodb.client.MongoClient;
import com.mongodb.client.MongoClients;
import com.mongodb.client.MongoDatabase;
import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import org.bson.Document;

/**
 * The live-network store: the official MongoDB sync driver behind the
 * {@link TsMongoStore} seam. Built only when the optional driver dependency is
 * present, and excluded from coverage - it is the one class that needs a running
 * server. The driver owns connection handling and SCRAM authentication; this is
 * just the thin map from the seam's methods onto its API.
 */
public final class DriverMongoStore implements TsMongoStore, AutoCloseable {

    private final MongoClient client;
    private final MongoDatabase db;

    public DriverMongoStore(String uri, String db) {
        this.client = MongoClients.create(uri);
        this.db = client.getDatabase(db);
    }

    @Override
    public long insertMany(String collection, List<Document> docs) {
        if (docs.isEmpty()) {
            return 0;
        }
        return db.getCollection(collection).insertMany(docs).getInsertedIds().size();
    }

    @Override
    public List<Document> findAll(String collection) {
        return db.getCollection(collection).find().into(new ArrayList<>());
    }

    @Override
    public Optional<Document> findOne(String collection, Object id) {
        Document found = db.getCollection(collection).find(new Document("_id", id)).first();
        return Optional.ofNullable(found);
    }

    @Override
    public void createIndex(String collection, Document keys) {
        db.getCollection(collection).createIndex(keys);
    }

    @Override
    public List<String> collections() {
        return db.listCollectionNames().into(new ArrayList<>());
    }

    @Override
    public void close() {
        client.close();
    }
}
