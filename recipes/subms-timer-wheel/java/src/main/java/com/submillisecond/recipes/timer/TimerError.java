package com.submillisecond.recipes.timer;

/**
 * A schedule the wheel refused. {@code schedule} clamps instead of throwing;
 * {@code trySchedule} raises this.
 *
 * <p>Byte-equivalent to the Rust sibling {@code subms_timer_wheel::TimerError}.
 */
public final class TimerError extends RuntimeException {

    private static final long serialVersionUID = 1L;

    public enum Kind { DELAY_TOO_LONG }

    private final Kind kind;
    private final long delay;
    private final long max;

    private TimerError(Kind kind, long delay, long max) {
        super("delay " + delay + " ticks exceeds the wheel capacity of " + max + " ticks");
        this.kind = kind;
        this.delay = delay;
        this.max = max;
    }

    public static TimerError delayTooLong(long delay, long max) {
        return new TimerError(Kind.DELAY_TOO_LONG, delay, max);
    }

    public Kind kind() { return kind; }
    public long delay() { return delay; }
    public long max() { return max; }
}
