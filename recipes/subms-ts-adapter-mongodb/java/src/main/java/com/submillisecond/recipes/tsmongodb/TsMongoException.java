package com.submillisecond.recipes.tsmongodb;

/** Error surface for the MongoDB adapter. */
public final class TsMongoException extends RuntimeException {

    public enum Kind {
        MAPPING,
        BSON,
        STORE,
        CONFIG
    }

    private final Kind kind;

    private TsMongoException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsMongoException mapping(String message) {
        return new TsMongoException(Kind.MAPPING, message);
    }

    public static TsMongoException bson(String message) {
        return new TsMongoException(Kind.BSON, message);
    }

    public static TsMongoException store(String message) {
        return new TsMongoException(Kind.STORE, message);
    }

    public static TsMongoException config(String message) {
        return new TsMongoException(Kind.CONFIG, message);
    }
}
