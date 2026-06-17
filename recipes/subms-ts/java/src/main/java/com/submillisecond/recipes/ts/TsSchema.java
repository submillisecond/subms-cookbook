package com.submillisecond.recipes.ts;

import java.util.Optional;

/** Semantic shape of a series' values. */
public sealed interface TsSchema
        permits TsSchema.Anonymous, TsSchema.Numeric, TsSchema.Schemaless, TsSchema.Custom {

    record Anonymous() implements TsSchema {}

    record Numeric(Optional<String> unit, TsNumericKind kind) implements TsSchema {}

    record Schemaless() implements TsSchema {}

    record Custom(String typeName) implements TsSchema {}

    static TsSchema anonymous() {
        return new Anonymous();
    }

    static TsSchema numeric(String unit, TsNumericKind kind) {
        return new Numeric(Optional.ofNullable(unit), kind);
    }
}
