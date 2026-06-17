package com.submillisecond.recipes.timer.features;

/**
 * Cron-expression parse failure. The exception payload distinguishes
 * structural errors (wrong field count) from field-content errors
 * (out-of-range value, malformed range, etc.).
 */
public final class CronError extends RuntimeException {

    public enum Kind { WRONG_FIELD_COUNT, INVALID_FIELD }

    private final Kind kind;
    private final int fieldCount;
    private final String fieldName;
    private final String fieldRaw;

    private CronError(Kind kind, int fieldCount, String fieldName, String fieldRaw) {
        super(message(kind, fieldCount, fieldName, fieldRaw));
        this.kind = kind;
        this.fieldCount = fieldCount;
        this.fieldName = fieldName;
        this.fieldRaw = fieldRaw;
    }

    public static CronError wrongFieldCount(int count) {
        return new CronError(Kind.WRONG_FIELD_COUNT, count, null, null);
    }

    public static CronError invalidField(String name, String raw) {
        return new CronError(Kind.INVALID_FIELD, -1, name, raw);
    }

    public Kind kind() { return kind; }
    public int fieldCount() { return fieldCount; }
    public String fieldName() { return fieldName; }
    public String fieldRaw() { return fieldRaw; }

    private static String message(Kind kind, int count, String name, String raw) {
        if (kind == Kind.WRONG_FIELD_COUNT) {
            return "cron expression must have 5 fields, got " + count;
        }
        return "invalid " + name + " field: " + raw;
    }
}
