package com.submillisecond.recipes.tsparquet;

import org.apache.parquet.hadoop.ParquetWriter;

/**
 * Writer sizing for the Parquet encode path: the column page size, the row-group
 * size, and whether dictionary encoding is on. Page / row-group sizing controls
 * how much buffer the writer allocates and how a reader pages the file, so it
 * should track the data rather than sit at a fixed value.
 *
 * <p>Use {@link #forPointCount(long)} (the default the no-arg encoders apply) to
 * size the page to the actual data, {@link #defaults()} for parquet-mr's standard
 * sizing (best for large files), or {@link #of(int, long)} / the {@code with*}
 * methods to pin exact values.
 */
public record TsParquetWriteOptions(int pageSize, long rowGroupSize, boolean dictionary) {

    /** Floor for an adaptively sized page - below this, page overhead dominates. */
    public static final int MIN_PAGE_SIZE = 8 * 1024;

    public TsParquetWriteOptions {
        if (pageSize <= 0) {
            throw new IllegalArgumentException("pageSize must be > 0, was " + pageSize);
        }
        if (rowGroupSize <= 0) {
            throw new IllegalArgumentException("rowGroupSize must be > 0, was " + rowGroupSize);
        }
    }

    /** parquet-mr's standard sizing (1 MiB page, 128 MiB row group), dictionary on. */
    public static TsParquetWriteOptions defaults() {
        return new TsParquetWriteOptions(
                ParquetWriter.DEFAULT_PAGE_SIZE, ParquetWriter.DEFAULT_BLOCK_SIZE, true);
    }

    /** Explicit page + row-group sizing, dictionary on. */
    public static TsParquetWriteOptions of(int pageSize, long rowGroupSize) {
        return new TsParquetWriteOptions(pageSize, rowGroupSize, true);
    }

    /**
     * Size the page to the data: roughly the encoded footprint of {@code points}
     * (~16 bytes each: an int64 timestamp plus an f64 value), clamped to
     * {@link #MIN_PAGE_SIZE} below and parquet-mr's default page size above, with
     * the row group a few pages large. This keeps a modest series from allocating
     * a megabyte page while still letting a large file use full-size pages.
     */
    public static TsParquetWriteOptions forPointCount(long points) {
        long estBytes = Math.max(points, 1) * 16L;
        int page = (int) Math.min(Math.max(estBytes, MIN_PAGE_SIZE), ParquetWriter.DEFAULT_PAGE_SIZE);
        long rowGroup = Math.min(Math.max(estBytes * 4, 64L * 1024), ParquetWriter.DEFAULT_BLOCK_SIZE);
        return new TsParquetWriteOptions(page, rowGroup, true);
    }

    public TsParquetWriteOptions withPageSize(int newPageSize) {
        return new TsParquetWriteOptions(newPageSize, rowGroupSize, dictionary);
    }

    public TsParquetWriteOptions withRowGroupSize(long newRowGroupSize) {
        return new TsParquetWriteOptions(pageSize, newRowGroupSize, dictionary);
    }

    public TsParquetWriteOptions withDictionary(boolean enabled) {
        return new TsParquetWriteOptions(pageSize, rowGroupSize, enabled);
    }
}
