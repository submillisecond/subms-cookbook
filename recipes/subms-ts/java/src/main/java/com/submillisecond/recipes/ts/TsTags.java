package com.submillisecond.recipes.ts;

import java.util.TreeMap;

/**
 * Prometheus-style label set. Case-sensitive (labels are identifiers). A thin
 * {@code TreeMap<String, String>} subclass so the Rust {@code TsTags = BTreeMap}
 * alias maps to a concrete Java type carrying the same ordered semantics.
 */
public final class TsTags extends TreeMap<String, String> {

    public TsTags() {
        super();
    }

    public TsTags(java.util.Map<String, String> src) {
        super(src);
    }
}
