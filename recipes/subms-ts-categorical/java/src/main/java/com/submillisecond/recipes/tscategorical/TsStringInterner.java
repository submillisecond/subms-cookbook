package com.submillisecond.recipes.tscategorical;

import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;

/**
 * A stable {@code String -> int} table. The first sight of a string assigns it
 * the next dense id (0, 1, 2, ... in first-seen order); every later sight of
 * the same string returns that id. Ids are never reused and never change, so a
 * code captured early stays valid for the interner's lifetime.
 *
 * <p>Backed by a {@link HashMap} for the forward lookup and an
 * {@link ArrayList} for the reverse: {@link #resolve(int)} is an O(1) index,
 * {@link #intern(String)} is an amortised O(1) hash probe.
 *
 * <p>The table is exact and unbounded - it holds one {@code String} per
 * distinct value and grows with the distinct-string count. That is the right
 * call for the bounded-alphabet case this recipe targets (a {@code symbol} /
 * {@code region} / {@code status} column drawn from a small fixed set). An
 * unbounded distinct stream needs a bounded / evicting variant; see the recipe
 * writeup's non-claims.
 */
public final class TsStringInterner {

    private final Map<String, Integer> forward;
    private final List<String> reverse;

    public TsStringInterner() {
        this.forward = new HashMap<>();
        this.reverse = new ArrayList<>();
    }

    public TsStringInterner(int capacity) {
        this.forward = new HashMap<>(Math.max(1, capacity * 2));
        this.reverse = new ArrayList<>(Math.max(1, capacity));
    }

    /**
     * Return the stable id for {@code s}, assigning a fresh dense id on first
     * sight. A repeat of the same string always returns the same id.
     */
    public int intern(String s) {
        Integer existing = forward.get(s);
        if (existing != null) {
            return existing;
        }
        int id = reverse.size();
        reverse.add(s);
        forward.put(s, id);
        return id;
    }

    /** The string for {@code id}, or empty if no such id has been assigned. */
    public Optional<String> resolve(int id) {
        if (id < 0 || id >= reverse.size()) {
            return Optional.empty();
        }
        return Optional.of(reverse.get(id));
    }

    /** The id for {@code s} without assigning one, or empty if never interned. */
    public Optional<Integer> get(String s) {
        return Optional.ofNullable(forward.get(s));
    }

    public boolean contains(String s) {
        return forward.containsKey(s);
    }

    /**
     * Distinct strings interned so far. Also the next id that
     * {@link #intern(String)} would assign to a never-before-seen string.
     */
    public int size() {
        return reverse.size();
    }

    public boolean isEmpty() {
        return reverse.isEmpty();
    }

    /** The interned strings in id order: index {@code i} is the string with id {@code i}. */
    public List<String> strings() {
        return Collections.unmodifiableList(reverse);
    }
}
