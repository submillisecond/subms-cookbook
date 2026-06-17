package com.submillisecond.recipes.timer.features;

import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.TreeSet;

/**
 * Minimum-viable cron expression. 5 fields: minute (0-59), hour
 * (0-23), day-of-month (1-31), month (1-12), day-of-week (0-6,
 * Sunday=0).
 *
 * <p>Per field we support:
 * <ul>
 *   <li>{@code *}     - every value in the field's range</li>
 *   <li>{@code &#42;/N}  - every Nth value</li>
 *   <li>{@code a-b}   - inclusive range</li>
 *   <li>{@code a,b,c} - explicit list (entries may be ranges or steps)</li>
 *   <li>{@code a}     - single literal value</li>
 * </ul>
 *
 * <p>Not supported: L/W/? extensions, named months/days, a seconds
 * field, step modifiers attached to ranges.
 *
 * <p>Byte-equivalent to the Rust sibling
 * {@code subms_timer_wheel::CronSchedule}.
 */
public final class CronSchedule {

    private final int[] minutes;
    private final int[] hours;
    private final int[] daysOfMonth;
    private final int[] months;
    private final int[] daysOfWeek;

    private CronSchedule(int[] minutes, int[] hours, int[] daysOfMonth,
                         int[] months, int[] daysOfWeek) {
        this.minutes = minutes;
        this.hours = hours;
        this.daysOfMonth = daysOfMonth;
        this.months = months;
        this.daysOfWeek = daysOfWeek;
    }

    public static CronSchedule parse(String expr) {
        String[] fields = expr.trim().split("\\s+");
        if (fields.length != 5) throw CronError.wrongFieldCount(fields.length);
        return new CronSchedule(
            parseField(fields[0], 0, 59, "minute"),
            parseField(fields[1], 0, 23, "hour"),
            parseField(fields[2], 1, 31, "day-of-month"),
            parseField(fields[3], 1, 12, "month"),
            parseField(fields[4], 0, 6, "day-of-week")
        );
    }

    public int[] minutes() { return minutes.clone(); }
    public int[] hours() { return hours.clone(); }
    public int[] daysOfMonth() { return daysOfMonth.clone(); }
    public int[] months() { return months.clone(); }
    public int[] daysOfWeek() { return daysOfWeek.clone(); }

    /**
     * Smallest epoch-second {@code >= afterEpoch} that the schedule
     * fires on. Returns -1 if no firing exists within ~5 years
     * (defensive cap; real schedules fire within a year).
     */
    public long nextAfter(long afterEpoch) {
        long t = ((afterEpoch + 59) / 60) * 60;
        long cap = afterEpoch + 5L * 365L * 24L * 60L * 60L;
        while (t < cap) {
            int[] civil = civilFromEpoch(t);
            int month = civil[1], dom = civil[2], dow = civil[3], hour = civil[4], minute = civil[5];
            if (!contains(minutes, minute)) { t += 60; continue; }
            if (!contains(hours, hour)) { t += 60; continue; }
            if (!contains(months, month)) { t += 60; continue; }
            if (!contains(daysOfMonth, dom)) { t += 60; continue; }
            if (!contains(daysOfWeek, dow)) { t += 60; continue; }
            return t;
        }
        return -1L;
    }

    private static boolean contains(int[] arr, int v) {
        for (int x : arr) if (x == v) return true;
        return false;
    }

    private static int[] parseField(String s, int lo, int hi, String name) {
        TreeSet<Integer> out = new TreeSet<>();
        for (String part : s.split(",")) {
            String p = part.trim();
            if (p.isEmpty()) throw CronError.invalidField(name, s);
            if (p.startsWith("*/")) {
                int step = parseIntOrThrow(p.substring(2), name, s);
                if (step == 0) throw CronError.invalidField(name, s);
                for (int v = lo; v <= hi; v += step) out.add(v);
                continue;
            }
            if (p.equals("*")) {
                for (int v = lo; v <= hi; v++) out.add(v);
                continue;
            }
            int dash = p.indexOf('-');
            if (dash > 0) {
                int a = parseIntOrThrow(p.substring(0, dash), name, s);
                int b = parseIntOrThrow(p.substring(dash + 1), name, s);
                if (a < lo || b > hi || a > b) throw CronError.invalidField(name, s);
                for (int v = a; v <= b; v++) out.add(v);
                continue;
            }
            int v = parseIntOrThrow(p, name, s);
            if (v < lo || v > hi) throw CronError.invalidField(name, s);
            out.add(v);
        }
        int[] arr = new int[out.size()];
        int i = 0;
        for (int v : out) arr[i++] = v;
        return arr;
    }

    private static int parseIntOrThrow(String s, String name, String raw) {
        try {
            return Integer.parseInt(s);
        } catch (NumberFormatException e) {
            throw CronError.invalidField(name, raw);
        }
    }

    /** Returns {year, month, dom, dow, hour, minute} for epoch-seconds UTC. */
    static int[] civilFromEpoch(long epoch) {
        long daysSinceEpoch = Math.floorDiv(epoch, 86_400L);
        long secsToday = Math.floorMod(epoch, 86_400L);
        int hour = (int) (secsToday / 3600);
        int minute = (int) ((secsToday % 3600) / 60);
        int dow = (int) Math.floorMod(daysSinceEpoch + 4, 7);

        long z = daysSinceEpoch + 719_468L;
        long era = (z >= 0 ? z : z - 146_096L) / 146_097L;
        long doe = z - era * 146_097L;
        long yoe = (doe - doe / 1460L + doe / 36_524L - doe / 146_096L) / 365L;
        long y = yoe + era * 400L;
        long doy = doe - (365L * yoe + yoe / 4L - yoe / 100L);
        long mp = (5L * doy + 2L) / 153L;
        int dom = (int) (doy - (153L * mp + 2L) / 5L + 1L);
        int month = (int) (mp < 10 ? mp + 3 : mp - 9);
        int year = (int) (y + (month <= 2 ? 1 : 0));
        return new int[] {year, month, dom, dow, hour, minute};
    }
}
