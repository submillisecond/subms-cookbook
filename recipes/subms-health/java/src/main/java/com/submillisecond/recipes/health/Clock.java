package com.submillisecond.recipes.health;

import java.time.Instant;
import java.time.ZoneOffset;
import java.time.format.DateTimeFormatter;

/** Clock seam: wall-clock millis (staleness) + RFC3339 stamp (refreshed_at). */
public interface Clock {
    long nowMs();

    String nowRfc3339();

    final class SystemClock implements Clock {
        private static final DateTimeFormatter FMT =
                DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss'Z'").withZone(ZoneOffset.UTC);

        @Override
        public long nowMs() {
            return System.currentTimeMillis();
        }

        @Override
        public String nowRfc3339() {
            return FMT.format(Instant.now());
        }
    }

    final class FixedClock implements Clock {
        private long ms;
        private final String stamp;

        public FixedClock(long ms, String rfc3339) {
            this.ms = ms;
            this.stamp = rfc3339;
        }

        public void set(long ms) {
            this.ms = ms;
        }

        @Override
        public long nowMs() {
            return ms;
        }

        @Override
        public String nowRfc3339() {
            return stamp;
        }
    }
}
