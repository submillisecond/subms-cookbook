package com.submillisecond.recipes.tssql;

import java.util.List;
import java.util.Optional;
import java.util.TreeMap;

import com.submillisecond.recipes.ts.TsDataFrame;

/**
 * A name -&gt; {@link TsDataFrame} registry. A query's {@code FROM <name>}
 * resolves against this. The catalog owns its frames so a query reads them; the
 * same catalog can back many queries. Mirrors the Rust {@code TsSqlCatalog}.
 */
public final class TsSqlCatalog {

    private final TreeMap<String, TsDataFrame> tables = new TreeMap<>();

    public TsSqlCatalog() {}

    /** Register {@code frame} under {@code name}, replacing any prior binding. */
    public void register(String name, TsDataFrame frame) {
        tables.put(name, frame);
    }

    /** The frame bound to {@code name}, if any. */
    public Optional<TsDataFrame> table(String name) {
        return Optional.ofNullable(tables.get(name));
    }

    /** The registered table names, in sorted order. */
    public List<String> tableNames() {
        return List.copyOf(tables.keySet());
    }
}
