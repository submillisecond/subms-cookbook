package com.submillisecond.recipes.tscategorical;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.ts.TsColumn;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * A dictionary-encoded string column. {@code codes[i]} indexes into the
 * dictionary, so {@code dict[codes[i]]} is the i-th logical string. Equal
 * strings share one code, which is the whole point: a group-by / join over
 * {@link #codes()} compares ints, and a high-duplication column shrinks to one
 * {@code String} per distinct value plus a flat int per row.
 */
public final class TsDictColumn {

    private final int[] codes;
    private final List<String> dict;

    private TsDictColumn(int[] codes, List<String> dict) {
        this.codes = codes;
        this.dict = dict;
    }

    /**
     * Encode any list of strings. Distinct values enter the dictionary in
     * first-seen order; codes follow the input order.
     */
    public static TsDictColumn fromStrings(List<String> values) {
        TsStringInterner interner = new TsStringInterner();
        int[] codes = new int[values.size()];
        for (int i = 0; i < values.size(); i++) {
            codes[i] = interner.intern(values.get(i));
        }
        return new TsDictColumn(codes, new ArrayList<>(interner.strings()));
    }

    /**
     * Encode a string time series. The ts axis is discarded - this is a value
     * column optimizer; round-tripping via {@link #toSeries()} re-emits dense
     * {@code 0..len} timestamps, not the originals.
     */
    public static TsDictColumn encode(TsSeries<String> series) {
        TsStringInterner interner = new TsStringInterner();
        List<Integer> codeList = new ArrayList<>(series.size());
        for (TsPoint<String> p : series) {
            codeList.add(interner.intern(p.value()));
        }
        int[] codes = new int[codeList.size()];
        for (int i = 0; i < codes.length; i++) {
            codes[i] = codeList.get(i);
        }
        return new TsDictColumn(codes, new ArrayList<>(interner.strings()));
    }

    /** The i-th string, or empty when {@code i} is out of range. */
    public Optional<String> decodeAt(int i) {
        if (i < 0 || i >= codes.length) {
            return Optional.empty();
        }
        return Optional.of(dict.get(codes[i]));
    }

    /**
     * The per-row code array (a defensive copy). This is the key surface: a
     * downstream operator groups / joins on these ints instead of the strings.
     */
    public int[] codes() {
        return codes.clone();
    }

    /** The dictionary: {@code dict[code]} is the string for {@code code}. */
    public List<String> dict() {
        return Collections.unmodifiableList(dict);
    }

    /** Distinct strings in the column (the dictionary size). */
    public int cardinality() {
        return dict.size();
    }

    /** Logical row count (the code array length). */
    public int size() {
        return codes.length;
    }

    public boolean isEmpty() {
        return codes.length == 0;
    }

    /**
     * The string for {@code code} straight out of the dictionary, bypassing the
     * per-row indirection.
     */
    public Optional<String> lookup(int code) {
        if (code < 0 || code >= dict.size()) {
            return Optional.empty();
        }
        return Optional.of(dict.get(code));
    }

    /**
     * Decode back to a string series with dense {@code 0..len} timestamps. The
     * values round-trip the original column exactly; the ts axis does not (it
     * was never stored).
     */
    public TsSeries<String> toSeries() {
        TsSeries<String> s = TsSeries.withCapacity(codes.length);
        for (int i = 0; i < codes.length; i++) {
            s.push(i, dict.get(codes[i]));
        }
        return s;
    }

    /** Decode the whole column into a plain list of strings in row order. */
    public List<String> toList() {
        List<String> out = new ArrayList<>(codes.length);
        for (int code : codes) {
            out.add(dict.get(code));
        }
        return out;
    }

    /**
     * Bridge to the analytical layer: dictionary-encode a {@code TsColumn.Str}
     * taken off a {@code TsDataFrame}. Returns empty when the column is not a
     * string column, so a caller can probe a frame's column without matching
     * the variant by hand.
     */
    public static Optional<TsDictColumn> encodeColumn(TsColumn col) {
        return col.asStr().map(TsDictColumn::encode);
    }
}
