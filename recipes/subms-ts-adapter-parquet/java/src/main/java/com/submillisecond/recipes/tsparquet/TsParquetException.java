package com.submillisecond.recipes.tsparquet;

/** Error surface for the Parquet adapter. */
public final class TsParquetException extends RuntimeException {

    public enum Kind {
        SCHEMA,
        PARQUET
    }

    private final Kind kind;

    private TsParquetException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    public static TsParquetException schema(String message) {
        return new TsParquetException(Kind.SCHEMA, message);
    }

    public static TsParquetException parquet(String message) {
        return new TsParquetException(Kind.PARQUET, message);
    }
}
