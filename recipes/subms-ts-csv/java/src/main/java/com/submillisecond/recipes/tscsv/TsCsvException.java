package com.submillisecond.recipes.tscsv;

/**
 * Thrown by the readers. The writer is total - any frame serialises - so it
 * returns a {@link String}, never throws. Mirrors the Rust {@code TsCsvError}
 * enum: a {@link Kind} discriminator plus the offending context.
 */
public final class TsCsvException extends RuntimeException {

    /** Which read rule the input broke. */
    public enum Kind {
        /** A data row had a field count that did not match the header. */
        RAGGED_ROW,
        /** A quoted field was unterminated or had trailing chars after the quote. */
        BAD_QUOTING,
        /** A {@code tsColumn} cell did not parse as a {@code long}. */
        BAD_TIMESTAMP,
        /** {@code tsColumn} named a column the header does not contain. */
        UNKNOWN_TS_COLUMN,
        /** An NDJSON line was not a flat JSON object, or was malformed. */
        BAD_JSON
    }

    private final Kind kind;

    private TsCsvException(Kind kind, String message) {
        super(message);
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }

    static TsCsvException raggedRow(int row, int expected, int got) {
        return new TsCsvException(Kind.RAGGED_ROW,
                "ragged row " + row + ": expected " + expected + " fields, got " + got);
    }

    static TsCsvException badQuoting(int row) {
        return new TsCsvException(Kind.BAD_QUOTING, "bad quoting in row " + row);
    }

    static TsCsvException badTimestamp(int row, String value) {
        return new TsCsvException(Kind.BAD_TIMESTAMP,
                "ts column value in row " + row + " is not a long: \"" + value + "\"");
    }

    static TsCsvException unknownTsColumn(String name) {
        return new TsCsvException(Kind.UNKNOWN_TS_COLUMN, "unknown ts column: " + name);
    }

    static TsCsvException badJson(int line, String hint) {
        return new TsCsvException(Kind.BAD_JSON, "bad json on line " + line + ": " + hint);
    }
}
