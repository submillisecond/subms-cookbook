package com.submillisecond.recipes.tsmongodb;

import org.bson.Document;

/**
 * A captured change. The in-memory store records one per inserted document; a
 * live driver would surface these off a MongoDB change stream.
 */
public record TsChangeEvent(String collection, Document doc) {

    public static TsChangeEvent insert(String collection, Document doc) {
        return new TsChangeEvent(collection, doc);
    }
}
