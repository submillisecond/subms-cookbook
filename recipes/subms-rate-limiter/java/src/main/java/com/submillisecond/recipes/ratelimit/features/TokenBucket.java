package com.submillisecond.recipes.ratelimit.features;

import java.math.BigInteger;

/**
 * Classic token bucket: capacity {@code C}, refill rate {@code R}
 * tokens per second. Tokens accumulate up to {@code C};
 * {@link #tryAcquire(long)} drains {@code n} tokens and succeeds when
 * {@code tokens >= n}.
 *
 * <p>Different shape from the base GCRA / leaky-bucket: callers can
 * drain a variable batch in a single call, and the bucket can sit at
 * full capacity through periods of inactivity. Useful when a request
 * weight varies per-call.
 *
 * <p>State is held under a single {@code synchronized} block for
 * correctness across the {@code acquire(n)} + refill path.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_rate_limiter::TokenBucket}.
 */
public final class TokenBucket {

    private static final BigInteger SCALE = BigInteger.valueOf(1_000_000_000L);

    private final long capacity;
    private final double ratePerSec;
    private final BigInteger unitsPerNsNum;
    private final BigInteger unitsPerNsDen;
    private final Clock clock;

    // Scaled tokens (1 token = 1_000_000_000 units); held in BigInteger
    // so multi-second accumulation doesn't overflow long.
    private BigInteger tokensScaled;
    private long lastNs;

    public TokenBucket(long capacity, double ratePerSec) {
        this(capacity, ratePerSec, new SystemClock());
    }

    public TokenBucket(long capacity, double ratePerSec, Clock clock) {
        this.capacity = Math.max(1L, capacity);
        this.ratePerSec = Math.max(0.0, ratePerSec);
        // units_per_ns = (rate * SCALE) / 1_000_000_000. Stored as a
        // rational; hot path stays in integer arithmetic.
        this.unitsPerNsNum = BigInteger.valueOf((long) (this.ratePerSec * 1_000_000_000.0));
        this.unitsPerNsDen = BigInteger.valueOf(1_000_000_000L);
        this.clock = clock;
        this.lastNs = clock.nowNs();
        this.tokensScaled = BigInteger.valueOf(this.capacity).multiply(SCALE);
    }

    /** Try to drain {@code n} tokens. Returns {@code true} if granted. */
    public synchronized boolean tryAcquire(long n) {
        if (n == 0L) return true;
        if (n < 0L) throw new IllegalArgumentException("n must be >= 0");
        refillLocked();
        BigInteger want = BigInteger.valueOf(n).multiply(SCALE);
        if (tokensScaled.compareTo(want) >= 0) {
            tokensScaled = tokensScaled.subtract(want);
            return true;
        }
        return false;
    }

    /** Shorthand for {@link #tryAcquire(long) tryAcquire(1)}. */
    public boolean tryAcquireOne() {
        return tryAcquire(1L);
    }

    /** Current token count (whole tokens; fractional units truncated). */
    public synchronized long available() {
        refillLocked();
        return tokensScaled.divide(SCALE).longValueExact();
    }

    public long capacity() {
        return capacity;
    }

    public double ratePerSec() {
        return ratePerSec;
    }

    private void refillLocked() {
        long now = clock.nowNs();
        long elapsed = now - lastNs;
        if (elapsed <= 0L) return;
        BigInteger add = BigInteger.valueOf(elapsed).multiply(unitsPerNsNum).divide(unitsPerNsDen);
        BigInteger capScaled = BigInteger.valueOf(capacity).multiply(SCALE);
        tokensScaled = tokensScaled.add(add).min(capScaled);
        lastNs = now;
    }
}
