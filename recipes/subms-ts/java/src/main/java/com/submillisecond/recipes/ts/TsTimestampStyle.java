package com.submillisecond.recipes.ts;

/** How timestamps render in a human-readable codec. */
public enum TsTimestampStyle {
    EPOCH_NANOS,
    EPOCH_MILLIS,
    /**
     * {@code YYYY-MM-DDTHH:MM:SS.fffffffffZ}. Encode-only in 0.6; decoding ISO
     * timestamps arrives with the {@code datetime} surface.
     */
    ISO8601
}
