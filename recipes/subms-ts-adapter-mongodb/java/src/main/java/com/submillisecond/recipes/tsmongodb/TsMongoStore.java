package com.submillisecond.recipes.tsmongodb;

import java.util.List;
import java.util.Optional;
import org.bson.Document;

/**
 * The minimal document-store surface the adapter needs. A live driver impl
 * ({@link DriverMongoStore}) is an optional dependency; tests use
 * {@link InMemoryMongoStore}.
 */
public interface TsMongoStore {

    long insertMany(String collection, List<Document> docs);

    List<Document> findAll(String collection);

    Optional<Document> findOne(String collection, Object id);

    void createIndex(String collection, Document keys);

    List<String> collections();

    /**
     * Drain captured change events. Default: none (a driver wires this to a
     * change stream, the live-network path excluded from coverage).
     */
    default List<TsChangeEvent> drainChanges() {
        return List.of();
    }
}
