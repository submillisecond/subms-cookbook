package com.submillisecond.recipes.ts;

public final class TsCollectionException extends RuntimeException {

    public enum Kind {
        DUPLICATE_ID,
        DUPLICATE_NAME,
        UNKNOWN_ID,
        INGEST
    }

    private final Kind kind;

    private TsCollectionException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    static TsCollectionException duplicateId(long id) {
        return new TsCollectionException(Kind.DUPLICATE_ID, "duplicate series id " + id);
    }

    static TsCollectionException duplicateName(String name) {
        return new TsCollectionException(Kind.DUPLICATE_NAME, "duplicate series name " + name);
    }

    static TsCollectionException unknownId(long id) {
        return new TsCollectionException(Kind.UNKNOWN_ID, "unknown series id " + id);
    }

    static TsCollectionException ingest(TsException e) {
        return new TsCollectionException(Kind.INGEST, e.getMessage());
    }

    public Kind kind() {
        return kind;
    }
}
