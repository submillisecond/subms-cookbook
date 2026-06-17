package com.submillisecond.recipes.ts;

import java.util.Collections;
import java.util.Locale;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;

/**
 * Free-form attribute bag with canonicalising inserts: key and value are
 * trimmed and ASCII-lowercased so query-by-attribute is canonical without the
 * caller doing string hygiene. Non-ASCII keys are rejected rather than mangled
 * by ambiguous Unicode case folding.
 */
public final class TsAttrs {

    private final TreeMap<String, String> map = new TreeMap<>();

    public TsAttrs() {}

    /**
     * Insert a normalised key=value.
     *
     * @throws TsAttrException if the key contains non-ASCII bytes (case folding
     *                         is ambiguous there - reject, don't guess).
     */
    public void insert(String key, String value) {
        String k = key.trim();
        if (!isAscii(k)) {
            throw new TsAttrException(k);
        }
        map.put(k.toLowerCase(Locale.ROOT), value.trim().toLowerCase(Locale.ROOT));
    }

    public Optional<String> get(String key) {
        return Optional.ofNullable(map.get(key.trim().toLowerCase(Locale.ROOT)));
    }

    public Optional<String> remove(String key) {
        return Optional.ofNullable(map.remove(key.trim().toLowerCase(Locale.ROOT)));
    }

    public Map<String, String> asMap() {
        return Collections.unmodifiableMap(map);
    }

    public int size() {
        return map.size();
    }

    public boolean isEmpty() {
        return map.isEmpty();
    }

    /** Does this attribute set match key=value after normalising both? */
    public boolean matches(String key, String value) {
        String want = value.trim().toLowerCase(Locale.ROOT);
        return get(key).map(want::equals).orElse(false);
    }

    @Override
    public boolean equals(Object o) {
        return o instanceof TsAttrs a && map.equals(a.map);
    }

    @Override
    public int hashCode() {
        return map.hashCode();
    }

    private static boolean isAscii(String s) {
        for (int i = 0; i < s.length(); i++) {
            if (s.charAt(i) > 0x7f) return false;
        }
        return true;
    }

    /** Thrown when a non-ASCII attribute key is inserted; carries the key. */
    public static final class TsAttrException extends RuntimeException {
        private final String key;

        TsAttrException(String key) {
            super("non-ASCII attribute key rejected: " + key);
            this.key = key;
        }

        public String key() {
            return key;
        }
    }
}
