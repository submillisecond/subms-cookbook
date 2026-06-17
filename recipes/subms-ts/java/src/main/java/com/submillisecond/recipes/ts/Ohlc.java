package com.submillisecond.recipes.ts;

/**
 * An OHLCV bar - the canonical five fields. Extra provider-specific fields
 * (adjusted close, open interest, ...) belong in a consumer's own value type;
 * the library ships the common shape plus the genericity to bring your own.
 */
public record Ohlc(double open, double high, double low, double close, double volume)
        implements TsValueKind {

    public static Ohlc of(double open, double high, double low, double close, double volume) {
        return new Ohlc(open, high, low, close, volume);
    }

    @Override
    public boolean tsIsPresent() {
        return Double.isFinite(open)
                && Double.isFinite(high)
                && Double.isFinite(low)
                && Double.isFinite(close)
                && Double.isFinite(volume);
    }
}
