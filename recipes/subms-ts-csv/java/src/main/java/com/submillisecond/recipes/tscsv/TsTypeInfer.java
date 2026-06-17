package com.submillisecond.recipes.tscsv;

import java.util.List;

/**
 * Per-column narrowest-fit type inference. Scan a column's non-empty cells and
 * pick the tightest element type that fits ALL of them: {@code I64} if every
 * cell parses as a {@code long}, else {@code F64} if every cell parses as a
 * finite {@code double}, else {@code BOOL} if every cell is {@code true} /
 * {@code false} (ASCII-case-insensitive), else {@code STR}. An empty (no-cell)
 * column infers {@code STR}.
 */
final class TsTypeInfer {

    private TsTypeInfer() {}

    /** Does this cell parse as a {@code long}? Matches {@link Long#parseLong}
     *  exactly so the inference-time check cannot disagree with the build-time
     *  parse: leading sign, ASCII digits, no whitespace, in {@code long} range. */
    static boolean isLong(String cell) {
        try {
            Long.parseLong(cell);
            return true;
        } catch (NumberFormatException e) {
            return false;
        }
    }

    /** Does this cell parse as a finite {@code double}? A token that parses to
     *  an infinity or NaN ("Infinity", "NaN") is rejected so the column falls
     *  through to {@code STR}, preserving the literal rather than degrading it
     *  to a gap. Java's parser also accepts a trailing {@code d}/{@code f} and
     *  a leading/trailing whitespace; we reject those so the grammar tracks the
     *  Rust {@code str::parse::<f64>} surface. */
    static boolean isDouble(String cell) {
        if (cell.isEmpty()) {
            return false;
        }
        for (int i = 0; i < cell.length(); i++) {
            char c = cell.charAt(i);
            boolean ok = (c >= '0' && c <= '9') || c == '+' || c == '-'
                    || c == '.' || c == 'e' || c == 'E';
            if (!ok) {
                return false;
            }
        }
        try {
            double v = Double.parseDouble(cell);
            return Double.isFinite(v);
        } catch (NumberFormatException e) {
            return false;
        }
    }

    static boolean isBool(String cell) {
        return cell.equalsIgnoreCase("true") || cell.equalsIgnoreCase("false");
    }

    /** Infer the column type from its non-empty cells. Single pass: track the
     *  all-long / all-double / all-bool invariants; return the tightest still
     *  standing. A long also satisfies double, so the long flag is the strict
     *  narrowing. */
    static TsInferredType infer(List<String> cells) {
        boolean any = false;
        boolean allLong = true;
        boolean allDouble = true;
        boolean allBool = true;

        for (String cell : cells) {
            any = true;
            if (allLong && !isLong(cell)) {
                allLong = false;
            }
            if (allDouble && !isDouble(cell)) {
                allDouble = false;
            }
            if (allBool && !isBool(cell)) {
                allBool = false;
            }
            if (!allLong && !allDouble && !allBool) {
                return TsInferredType.STR;
            }
        }

        if (!any) {
            return TsInferredType.STR;
        }
        if (allLong) {
            return TsInferredType.I64;
        }
        if (allDouble) {
            return TsInferredType.F64;
        }
        if (allBool) {
            return TsInferredType.BOOL;
        }
        return TsInferredType.STR;
    }
}
