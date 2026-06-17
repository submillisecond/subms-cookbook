package com.submillisecond.recipes.tswal;

/**
 * When to {@code force} the active segment to disk.
 *
 * <p>{@link #ALWAYS} forces on every append: durable per-record, but
 * fsync-floor-limited and NOT a sub-ms guarantee on slow storage. The batched
 * policies amortise the force across many appends and are what the recipe's
 * sub-ms claim covers. {@link #NEVER} leaves durability to {@code flush()} /
 * close.
 *
 * <p>Mirrors the Rust {@code TsFsyncPolicy} enum: a single kind plus an integer
 * argument, constructed via the {@link #everyNAppends(int)} /
 * {@link #everyNMillis(int)} factories.
 */
public final class TsFsyncPolicy {

    /** Force on every append. */
    public static final TsFsyncPolicy ALWAYS = new TsFsyncPolicy(Kind.ALWAYS, 0);
    /** Never force automatically; rely on flush()/close. */
    public static final TsFsyncPolicy NEVER = new TsFsyncPolicy(Kind.NEVER, 0);

    enum Kind {
        ALWAYS,
        EVERY_N_APPENDS,
        EVERY_N_MILLIS,
        NEVER
    }

    private final Kind kind;
    private final int arg;

    private TsFsyncPolicy(Kind kind, int arg) {
        this.kind = kind;
        this.arg = arg;
    }

    /** Force once every {@code n} appends. */
    public static TsFsyncPolicy everyNAppends(int n) {
        return new TsFsyncPolicy(Kind.EVERY_N_APPENDS, n);
    }

    /** Force at most once every {@code ms} milliseconds. */
    public static TsFsyncPolicy everyNMillis(int ms) {
        return new TsFsyncPolicy(Kind.EVERY_N_MILLIS, ms);
    }

    Kind kind() {
        return kind;
    }

    int arg() {
        return arg;
    }
}
