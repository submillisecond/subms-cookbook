package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.Optional;

/**
 * Identity + schema + relationship block for a series. Optional on a
 * {@link TsSeries}; required when the series lives in a {@link TsCollection}
 * registry. Mutable builder-style, mirroring the Rust struct + with_* chain.
 */
public final class TsSeriesMetadata {

    private long id;
    private String name;
    private TsSchema schema = TsSchema.anonymous();
    private Optional<TsFormat> format = Optional.empty();
    private final TsTags tags = new TsTags();
    private final TsAttrs attributes = new TsAttrs();
    private final List<TsDep> dependencies = new ArrayList<>();

    public TsSeriesMetadata(long id, String name) {
        this.id = id;
        this.name = Objects.requireNonNull(name, "name");
    }

    public static TsSeriesMetadata of(long id, String name) {
        return new TsSeriesMetadata(id, name);
    }

    public long id() {
        return id;
    }

    public String name() {
        return name;
    }

    public TsSchema schema() {
        return schema;
    }

    public Optional<TsFormat> format() {
        return format;
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

    public TsSeriesMetadata withSchema(TsSchema schema) {
        this.schema = schema;
        return this;
    }

    public TsSeriesMetadata withFormat(TsFormat format) {
        this.format = Optional.of(format);
        return this;
    }

    public TsSeriesMetadata withTag(String key, String value) {
        tags.put(key, value);
        return this;
    }

    public TsSeriesMetadata withDependency(TsDep dep) {
        dependencies.add(dep);
        return this;
    }

    /** True if every key=value in {@code want} matches this series' tags. */
    public boolean hasTags(Map<String, String> want) {
        for (Map.Entry<String, String> e : want.entrySet()) {
            if (!Objects.equals(tags.get(e.getKey()), e.getValue())) return false;
        }
        return true;
    }
}
