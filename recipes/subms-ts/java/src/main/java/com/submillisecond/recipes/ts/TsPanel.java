package com.submillisecond.recipes.ts;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;
import java.util.function.BiPredicate;

/**
 * A homogeneous, single-{@code T}, ts-aligned multi-series container (an OHLCV
 * bar set, an order book, a multi-symbol basket). Series live in named slots;
 * {@link #aligned} walks every slot in lock-step by ts. Every column shares
 * one type; for a heterogeneous, per-column-typed container see
 * {@link TsDataFrame}.
 */
public final class TsPanel<T> {

    private static final class Slot<T> {
        final String name;
        TsSeries<T> series;

        Slot(String name, TsSeries<T> series) {
            this.name = name;
            this.series = series;
        }
    }

    private final TsPanelMetadata meta;
    private final List<Slot<T>> slots = new ArrayList<>();
    private final List<TsPanelGroup> groups = new ArrayList<>();

    public TsPanel(TsPanelMetadata meta) {
        this.meta = meta;
    }

    public TsPanelMetadata metadata() {
        return meta;
    }

    /** Add or replace a slot. Insertion order is preserved and defines the
     *  column order of {@link #aligned} rows. */
    public void addSeries(String slotName, TsSeries<T> series) {
        for (Slot<T> s : slots) {
            if (s.name.equals(slotName)) {
                s.series = series;
                return;
            }
        }
        slots.add(new Slot<>(slotName, series));
    }

    public Optional<TsSeries<T>> series(String slotName) {
        for (Slot<T> s : slots) {
            if (s.name.equals(slotName)) return Optional.of(s.series);
        }
        return Optional.empty();
    }

    public List<String> slotNames() {
        List<String> out = new ArrayList<>(slots.size());
        for (Slot<T> s : slots) out.add(s.name);
        return out;
    }

    public int size() {
        return slots.size();
    }

    public boolean isEmpty() {
        return slots.isEmpty();
    }

    public void addGroup(TsPanelGroup group) {
        groups.add(group);
    }

    public Optional<TsPanelGroup> group(String groupName) {
        for (TsPanelGroup g : groups) {
            if (g.name().equals(groupName)) return Optional.of(g);
        }
        return Optional.empty();
    }

    public List<String> seriesInGroup(String groupName) {
        List<String> members = group(groupName).map(TsPanelGroup::seriesNames).orElse(List.of());
        List<String> out = new ArrayList<>();
        for (Slot<T> s : slots) {
            if (members.contains(s.name)) out.add(s.name);
        }
        return out;
    }

    /** Lock-step view over every slot by ts. */
    public TsPanelAligned<T> aligned() {
        List<List<TsPoint<T>>> columns = new ArrayList<>(slots.size());
        for (Slot<T> s : slots) {
            List<TsPoint<T>> col = new ArrayList<>();
            for (TsPoint<T> p : s.series) col.add(p);
            columns.add(col);
        }
        return new TsPanelAligned<>(columns);
    }

    // ---------- delete surface ----------

    public Optional<TsSeries<T>> drop(String slotName) {
        for (int i = 0; i < slots.size(); i++) {
            if (slots.get(i).name.equals(slotName)) {
                return Optional.of(slots.remove(i).series);
            }
        }
        return Optional.empty();
    }

    public Optional<TsPanelGroup> removeGroup(String groupName) {
        for (int i = 0; i < groups.size(); i++) {
            if (groups.get(i).name().equals(groupName)) {
                return Optional.of(groups.remove(i));
            }
        }
        return Optional.empty();
    }

    public Optional<TsPoint<T>> deleteAt(String slotName, long ts) {
        return series(slotName).flatMap(s -> s.deleteAt(ts));
    }

    public int deleteRange(String slotName, long lo, long hi) {
        return series(slotName).map(s -> s.deleteRange(lo, hi)).orElse(0);
    }

    public int truncateBefore(long cutoff) {
        int n = 0;
        for (Slot<T> s : slots) n += s.series.truncateBefore(cutoff);
        return n;
    }

    public int truncateAfter(long cutoff) {
        int n = 0;
        for (Slot<T> s : slots) n += s.series.truncateAfter(cutoff);
        return n;
    }

    public int retainSlots(BiPredicate<String, TsSeries<T>> keep) {
        int before = slots.size();
        slots.removeIf(s -> !keep.test(s.name, s.series));
        return before - slots.size();
    }

    public void clear() {
        slots.clear();
        groups.clear();
    }
}
