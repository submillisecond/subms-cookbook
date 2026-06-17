package com.submillisecond.recipes.tsinfluxdb;

import java.util.OptionalLong;

/**
 * RFC3339 (UTC) to/from epoch-nanoseconds, JDK-only. Influx writes accept
 * integer nanosecond timestamps directly, but a Flux response carries
 * {@code _time} as an RFC3339 string in UTC with a trailing {@code Z} and an
 * optional fractional part - a fixed-shape scan, not a calendar library. Byte
 * for byte equivalent to the Rust sibling's time module.
 */
final class Rfc3339 {

    private static final int[] DAYS_IN_MONTH = {31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31};

    private Rfc3339() {}

    private static boolean isLeap(long year) {
        return (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
    }

    private static long daysFromCivil(long year, long month, long day) {
        long y = month <= 2 ? year - 1 : year;
        long era = (y >= 0 ? y : y - 399) / 400;
        long yoe = y - era * 400;
        long doy = (153 * (month > 2 ? month - 3 : month + 9) + 2) / 5 + day - 1;
        long doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        return era * 146097 + doe - 719468;
    }

    private static long[] civilFromDays(long z) {
        z += 719468;
        long era = (z >= 0 ? z : z - 146096) / 146097;
        long doe = z - era * 146097;
        long yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        long y = yoe + era * 400;
        long doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        long mp = (5 * doy + 2) / 153;
        long d = doy - (153 * mp + 2) / 5 + 1;
        long m = mp < 10 ? mp + 3 : mp - 9;
        return new long[] {m <= 2 ? y + 1 : y, m, d};
    }

    /** Parse an RFC3339 UTC timestamp into epoch nanoseconds, or empty. */
    static OptionalLong parseNanos(String s) {
        byte[] b = s.getBytes(java.nio.charset.StandardCharsets.US_ASCII);
        if (b.length < 20) {
            return OptionalLong.empty();
        }
        if (b[4] != '-' || b[7] != '-' || b[10] != 'T' || b[13] != ':' || b[16] != ':') {
            return OptionalLong.empty();
        }
        long year = num(b, 0, 4);
        long month = num(b, 5, 7);
        long day = num(b, 8, 10);
        long hour = num(b, 11, 13);
        long min = num(b, 14, 16);
        long sec = num(b, 17, 19);
        if (year < 0 || month < 0 || day < 0 || hour < 0 || min < 0 || sec < 0) {
            return OptionalLong.empty();
        }
        if (month < 1 || month > 12 || day < 1 || hour > 23 || min > 59 || sec > 60) {
            return OptionalLong.empty();
        }
        long maxDay = DAYS_IN_MONTH[(int) (month - 1)] + (month == 2 && isLeap(year) ? 1 : 0);
        if (day > maxDay) {
            return OptionalLong.empty();
        }

        int idx = 19;
        long fracNanos = 0;
        if (idx < b.length && b[idx] == '.') {
            idx++;
            int start = idx;
            while (idx < b.length && b[idx] >= '0' && b[idx] <= '9') {
                idx++;
            }
            if (idx == start) {
                return OptionalLong.empty();
            }
            StringBuilder scaled = new StringBuilder(9);
            for (int i = 0; i < 9; i++) {
                int p = start + i;
                scaled.append(p < idx ? (char) b[p] : '0');
            }
            fracNanos = Long.parseLong(scaled.toString());
        }
        if (idx >= b.length || b[idx] != 'Z' || idx + 1 != b.length) {
            return OptionalLong.empty();
        }

        long days = daysFromCivil(year, month, day);
        long secs = days * 86_400 + hour * 3_600 + min * 60 + sec;
        return OptionalLong.of(secs * 1_000_000_000L + fracNanos);
    }

    /** Format epoch nanoseconds as an RFC3339 UTC timestamp. */
    static String formatNanos(long nanos) {
        long secs = Math.floorDiv(nanos, 1_000_000_000L);
        long frac = Math.floorMod(nanos, 1_000_000_000L);
        long days = Math.floorDiv(secs, 86_400L);
        secs = Math.floorMod(secs, 86_400L);
        long[] ymd = civilFromDays(days);
        long hh = secs / 3_600;
        long mm = (secs % 3_600) / 60;
        long ss = secs % 60;
        if (frac == 0) {
            return String.format("%04d-%02d-%02dT%02d:%02d:%02dZ", ymd[0], ymd[1], ymd[2], hh, mm, ss);
        }
        return String.format(
                "%04d-%02d-%02dT%02d:%02d:%02d.%09dZ", ymd[0], ymd[1], ymd[2], hh, mm, ss, frac);
    }

    private static long num(byte[] b, int lo, int hi) {
        long v = 0;
        for (int i = lo; i < hi; i++) {
            if (b[i] < '0' || b[i] > '9') {
                return -1;
            }
            v = v * 10 + (b[i] - '0');
        }
        return v;
    }
}
