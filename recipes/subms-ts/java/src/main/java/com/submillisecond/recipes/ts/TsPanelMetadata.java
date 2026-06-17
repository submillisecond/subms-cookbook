package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.List;
import java.util.Objects;

public final class TsPanelMetadata {

    private final String name;
    private final TsTags tags = new TsTags();
    private final TsAttrs attributes = new TsAttrs();
    private final List<TsDep> dependencies = new ArrayList<>();

    public TsPanelMetadata(String name) {
        this.name = Objects.requireNonNull(name, "name");
    }

    public static TsPanelMetadata of(String name) {
        return new TsPanelMetadata(name);
    }

    public String name() {
        return name;
    }

    public TsTags tags() {
        return tags;
    }

    public TsAttrs attributes() {
        return attributes;
    }

    public List<TsDep> dependencies() {
        return dependencies;
    }
}
