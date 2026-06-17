package com.submillisecond.recipes.timer.features;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class CronScheduleTest {

    @Test
    void parseStarFieldExpandsToFullRange() {
        CronSchedule s = CronSchedule.parse("* * * * *");
        assertEquals(60, s.minutes().length);
        assertEquals(24, s.hours().length);
        assertEquals(31, s.daysOfMonth().length);
        assertEquals(12, s.months().length);
        assertEquals(7, s.daysOfWeek().length);
    }

    @Test
    void parseStepExpressionPicksCorrectMinutes() {
        CronSchedule s = CronSchedule.parse("*/15 * * * *");
        assertArrayEquals(new int[] {0, 15, 30, 45}, s.minutes());
    }

    @Test
    void parseListAndRangeCombine() {
        CronSchedule s = CronSchedule.parse("0,30 1-3 * * *");
        assertArrayEquals(new int[] {0, 30}, s.minutes());
        assertArrayEquals(new int[] {1, 2, 3}, s.hours());
    }

    @Test
    void parseLiteralValue() {
        CronSchedule s = CronSchedule.parse("15 14 1 1 *");
        assertArrayEquals(new int[] {15}, s.minutes());
        assertArrayEquals(new int[] {14}, s.hours());
        assertArrayEquals(new int[] {1}, s.daysOfMonth());
        assertArrayEquals(new int[] {1}, s.months());
    }

    @Test
    void parseRejectsWrongFieldCount() {
        CronError e1 = assertThrows(CronError.class, () -> CronSchedule.parse("* * * *"));
        assertEquals(CronError.Kind.WRONG_FIELD_COUNT, e1.kind());
        assertEquals(4, e1.fieldCount());
        CronError e2 = assertThrows(CronError.class, () -> CronSchedule.parse("* * * * * *"));
        assertEquals(6, e2.fieldCount());
    }

    @Test
    void parseRejectsOutOfRangeMinute() {
        CronError e = assertThrows(CronError.class, () -> CronSchedule.parse("60 * * * *"));
        assertEquals(CronError.Kind.INVALID_FIELD, e.kind());
        assertEquals("minute", e.fieldName());
    }

    @Test
    void parseRejectsInvertedRange() {
        CronError e = assertThrows(CronError.class, () -> CronSchedule.parse("5-1 * * * *"));
        assertEquals(CronError.Kind.INVALID_FIELD, e.kind());
    }

    @Test
    void parseRejectsZeroStep() {
        CronError e = assertThrows(CronError.class, () -> CronSchedule.parse("*/0 * * * *"));
        assertEquals(CronError.Kind.INVALID_FIELD, e.kind());
    }

    @Test
    void parseRejectsNonNumericField() {
        CronError e = assertThrows(CronError.class, () -> CronSchedule.parse("abc * * * *"));
        assertEquals(CronError.Kind.INVALID_FIELD, e.kind());
    }

    @Test
    void parseRejectsEmptyListEntry() {
        CronError e = assertThrows(CronError.class, () -> CronSchedule.parse("1,,2 * * * *"));
        assertEquals(CronError.Kind.INVALID_FIELD, e.kind());
    }

    @Test
    void civilFromEpochReturnsKnownAnchor() {
        int[] c = CronSchedule.civilFromEpoch(1_704_067_200L);
        assertEquals(2024, c[0]);
        assertEquals(1, c[1]);
        assertEquals(1, c[2]);
        // Monday = 1.
        assertEquals(1, c[3]);
        assertEquals(0, c[4]);
        assertEquals(0, c[5]);
    }

    @Test
    void nextAfterForEveryMinute() {
        CronSchedule s = CronSchedule.parse("* * * * *");
        assertEquals(1_704_067_260L, s.nextAfter(1_704_067_230L));
    }

    @Test
    void nextAfterForEveryFiveMinutes() {
        CronSchedule s = CronSchedule.parse("*/5 * * * *");
        assertEquals(1_704_067_500L, s.nextAfter(1_704_067_201L));
    }

    @Test
    void nextAfterRespectsHourFilter() {
        CronSchedule s = CronSchedule.parse("0 14 * * *");
        long now = 1_704_067_200L + 13L * 3600L;
        assertEquals(1_704_067_200L + 14L * 3600L, s.nextAfter(now));
    }

    @Test
    void cronSchedulerAdvancesPastRecordedFire() {
        CronSchedule s = CronSchedule.parse("* * * * *");
        CronScheduler cs = new CronScheduler(s, 1_704_067_200L);
        long first = cs.nextFire(1_704_067_200L);
        assertEquals(1_704_067_260L, first);
        cs.recordFire(first);
        long second = cs.nextFire(first);
        assertEquals(first + 60L, second);
    }

    @Test
    void errorMessagesAreDescriptive() {
        CronError e1 = CronError.wrongFieldCount(3);
        assertTrue(e1.getMessage().contains("5 fields"));
        CronError e2 = CronError.invalidField("minute", "60");
        assertTrue(e2.getMessage().contains("minute"));
    }
}
