package com.submillisecond.recipes.ts;

import java.util.List;

/**
 * The ordered (name, type) shape of a {@link TsDataFrame}. Distinct from the
 * per-series {@link TsSchema}, which describes a single series' value layout;
 * this describes a whole frame's columns.
 */
public record TsFrameSchema(List<TsField> fields) {

    public TsFrameSchema {
        fields = List.copyOf(fields);
    }
}
