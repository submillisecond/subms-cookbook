package com.submillisecond.recipes.tsinfluxdb;

/**
 * Error surface for the InfluxDB adapter. Mirrors the Rust sibling's
 * {@code TsInfluxError} enum: the {@link Kind} discriminates encode / csv /
 * http / transport / config failures; {@link #status()} carries the HTTP code
 * for {@code HTTP} errors (0 otherwise).
 */
public final class TsInfluxException extends RuntimeException {

    public enum Kind { ENCODE, CSV, HTTP, TRANSPORT, CONFIG }

    private final Kind kind;
    private final int status;

    private TsInfluxException(Kind kind, int status, String message) {
        super(message);
        this.kind = kind;
        this.status = status;
    }

    public Kind kind() {
        return kind;
    }

    public int status() {
        return status;
    }

    public static TsInfluxException encode(String msg) {
        return new TsInfluxException(Kind.ENCODE, 0, "line-protocol encode error: " + msg);
    }

    public static TsInfluxException csv(String msg) {
        return new TsInfluxException(Kind.CSV, 0, "csv decode error: " + msg);
    }

    public static TsInfluxException http(int status, String body) {
        return new TsInfluxException(Kind.HTTP, status, "influx http " + status + ": " + body);
    }

    public static TsInfluxException transport(String msg) {
        return new TsInfluxException(Kind.TRANSPORT, 0, "transport error: " + msg);
    }

    public static TsInfluxException config(String msg) {
        return new TsInfluxException(Kind.CONFIG, 0, "config error: " + msg);
    }
}
