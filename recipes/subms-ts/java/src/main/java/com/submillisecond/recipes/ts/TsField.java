package com.submillisecond.recipes.ts;

/** One named, typed column slot in a {@link TsFrameSchema}. */
public record TsField(String name, TsDataType dataType) {}
