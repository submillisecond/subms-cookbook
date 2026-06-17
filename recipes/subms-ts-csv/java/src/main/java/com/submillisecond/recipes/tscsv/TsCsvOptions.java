package com.submillisecond.recipes.tscsv;

import java.util.Optional;

/**
 * Reader / writer options. {@code delimiter} is the CSV field separator
 * (default {@code ,}); {@code hasHeader} toggles whether the first row names
 * the columns (when {@code false}, columns are synthesised {@code col0..colN});
 * {@code tsColumn}, when set, names the column whose {@code long} cells become
 * the frame's timestamp axis (that column is consumed, not re-emitted).
 *
 * <p>Immutable; the chained setters return a fresh instance, mirroring the Rust
 * builder.
 */
public final class TsCsvOptions {

    private final boolean hasHeader;
    private final String tsColumn;
    private final char delimiter;

    private TsCsvOptions(boolean hasHeader, String tsColumn, char delimiter) {
        this.hasHeader = hasHeader;
        this.tsColumn = tsColumn;
        this.delimiter = delimiter;
    }

    /** Defaults: header present, no ts column, comma delimiter. */
    public static TsCsvOptions defaults() {
        return new TsCsvOptions(true, null, ',');
    }

    public TsCsvOptions hasHeader(boolean yes) {
        return new TsCsvOptions(yes, tsColumn, delimiter);
    }

    public TsCsvOptions tsColumn(String name) {
        return new TsCsvOptions(hasHeader, name, delimiter);
    }

    public TsCsvOptions delimiter(char delim) {
        return new TsCsvOptions(hasHeader, tsColumn, delim);
    }

    public boolean header() {
        return hasHeader;
    }

    public Optional<String> ts() {
        return Optional.ofNullable(tsColumn);
    }

    public char delim() {
        return delimiter;
    }
}
