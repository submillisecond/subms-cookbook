package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.function.Predicate;

/**
 * A flat by-id registry of many series. The multi-tenant scratch space: fast
 * lookup by id / name / tag, bulk delete by tag / predicate. For a coherent
 * multi-series concept (OHLCV bars, an order book) use {@link TsPanel} instead.
 */
public final class TsCollection<T> {

    private final Map<Long, TsSeries<T>> series = new HashMap<>();
    private final Map<String, Long> names = new HashMap<>();

    public TsCollection() {}

    /** Register an empty series under its metadata id + name. Returns the id. */
    public long register(TsSeriesMetadata meta) {
        long id = meta.id();
        if (series.containsKey(id)) {
            throw TsCollectionException.duplicateId(id);
        }
        if (!meta.name().isEmpty() && names.containsKey(meta.name())) {
            throw TsCollectionException.duplicateName(meta.name());
        }
        if (!meta.name().isEmpty()) {
            names.put(meta.name(), id);
        }
        series.put(id, new TsSeries<T>().withMetadata(meta));
        return id;
    }

    public void push(long id, long ts, T value) {
        TsSeries<T> s = series.get(id);
        if (s == null) throw TsCollectionException.unknownId(id);
        try {
            s.push(ts, value);
        } catch (TsException e) {
            throw TsCollectionException.ingest(e);
        }
    }

    public Optional<TsSeries<T>> get(long id) {
        return Optional.ofNullable(series.get(id));
    }

    public Optional<TsSeries<T>> byName(String name) {
        Long id = names.get(name);
        return id == null ? Optional.empty() : Optional.ofNullable(series.get(id));
    }

    public List<TsSeries<T>> byTag(String key, String value) {
        List<TsSeries<T>> out = new ArrayList<>();
        for (TsSeries<T> s : series.values()) {
            if (tagMatches(s, key, value)) out.add(s);
        }
        return out;
    }

    public List<Long> ids() {
        return new ArrayList<>(series.keySet());
    }

    public List<TsSeries<T>> series() {
        return new ArrayList<>(series.values());
    }

    public int size() {
        return series.size();
    }

    public boolean isEmpty() {
        return series.isEmpty();
    }

    public boolean contains(long id) {
        return series.containsKey(id);
    }

    // ---------- delete surface ----------

    public Optional<TsSeries<T>> deregister(long id) {
        TsSeries<T> s = series.remove(id);
        if (s == null) return Optional.empty();
        s.metadata().ifPresent(m -> names.remove(m.name()));
        return Optional.of(s);
    }

    public Optional<TsPoint<T>> deleteAt(long id, long ts) {
        TsSeries<T> s = series.get(id);
        return s == null ? Optional.empty() : s.deleteAt(ts);
    }

    public int deleteRange(long id, long lo, long hi) {
        TsSeries<T> s = series.get(id);
        return s == null ? 0 : s.deleteRange(lo, hi);
    }

    public int truncateBefore(long cutoff) {
        int n = 0;
        for (TsSeries<T> s : series.values()) n += s.truncateBefore(cutoff);
        return n;
    }

    public int truncateAfter(long cutoff) {
        int n = 0;
        for (TsSeries<T> s : series.values()) n += s.truncateAfter(cutoff);
        return n;
    }

    public int deleteAtByTag(String key, String value, long ts) {
        int n = 0;
        for (long id : matchingIds(key, value)) {
            TsSeries<T> s = series.get(id);
            if (s != null && s.deleteAt(ts).isPresent()) n++;
        }
        return n;
    }

    public int deleteRangeByTag(String key, String value, long lo, long hi) {
        int n = 0;
        for (long id : matchingIds(key, value)) {
            TsSeries<T> s = series.get(id);
            if (s != null) n += s.deleteRange(lo, hi);
        }
        return n;
    }

    public List<TsSeries<T>> evictByTag(String key, String value) {
        List<TsSeries<T>> out = new ArrayList<>();
        for (long id : matchingIds(key, value)) {
            deregister(id).ifPresent(out::add);
        }
        return out;
    }

    public List<TsSeries<T>> evictWhere(Predicate<TsSeriesMetadata> pred) {
        List<Long> ids = new ArrayList<>();
        for (Map.Entry<Long, TsSeries<T>> e : series.entrySet()) {
            if (e.getValue().metadata().map(pred::test).orElse(false)) ids.add(e.getKey());
        }
        List<TsSeries<T>> out = new ArrayList<>();
        for (long id : ids) {
            deregister(id).ifPresent(out::add);
        }
        return out;
    }

    public void clear() {
        series.clear();
        names.clear();
    }

    // ---------- numeric cross-series aggregation ----------

    /** Aggregate the latest value as-of {@code ts} (each series' nearestBefore). */
    public Optional<Double> aggregateAt(long ts, TsAgg agg, TsNumeric<T> num) {
        List<T> vals = new ArrayList<>();
        for (TsSeries<T> s : series.values()) {
            s.nearestBefore(ts).ifPresent(p -> vals.add(p.value()));
        }
        return foldValues(agg, num, vals);
    }

    /** Same, restricted to series matching key=value. */
    public Optional<Double> aggregateAtByTag(String key, String value, long ts, TsAgg agg, TsNumeric<T> num) {
        List<T> vals = new ArrayList<>();
        for (TsSeries<T> s : byTag(key, value)) {
            s.nearestBefore(ts).ifPresent(p -> vals.add(p.value()));
        }
        return foldValues(agg, num, vals);
    }

    private Optional<Double> foldValues(TsAgg agg, TsNumeric<T> num, List<T> vals) {
        if (vals.isEmpty()) return Optional.empty();
        T acc = num.zero();
        T min = null;
        T max = null;
        for (T v : vals) {
            acc = num.add(acc, v);
            if (min == null || num.compare(v, min) < 0) min = v;
            if (max == null || num.compare(v, max) > 0) max = v;
        }
        return Optional.of(switch (agg) {
            case SUM -> num.toDouble(acc);
            case MIN -> num.toDouble(min);
            case MAX -> num.toDouble(max);
            case MEAN -> num.toDouble(acc) / vals.size();
            case COUNT -> (double) vals.size();
        });
    }

    private List<Long> matchingIds(String key, String value) {
        List<Long> out = new ArrayList<>();
        for (Map.Entry<Long, TsSeries<T>> e : series.entrySet()) {
            if (tagMatches(e.getValue(), key, value)) out.add(e.getKey());
        }
        return out;
    }

    private boolean tagMatches(TsSeries<T> s, String key, String value) {
        return s.metadata()
                .map(m -> value.equals(m.tags().get(key)))
                .orElse(false);
    }
}
