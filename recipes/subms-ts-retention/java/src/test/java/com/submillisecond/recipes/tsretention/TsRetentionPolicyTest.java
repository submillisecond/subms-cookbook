package com.submillisecond.recipes.tsretention;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import java.util.List;
import java.util.OptionalInt;

import org.junit.jupiter.api.Test;

import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesL;

class TsRetentionPolicyTest {

    private static TsSeriesD series(int n) {
        TsSeriesD s = new TsSeriesD();
        for (int i = 0; i < n; i++) {
            s.push(i, (double) i);
        }
        return s;
    }

    @Test
    void emptyPolicyIsNoop() {
        TsSeriesD s = series(100);
        int removed = TsRetentionPolicy.create().apply(s);
        assertEquals(0, removed);
        assertEquals(100, s.size());
    }

    @Test
    void countKeepsNewest() {
        TsSeriesD s = series(1_000);
        int removed = TsRetentionPolicy.create().maxPoints(100).apply(s);
        assertEquals(900, removed);
        assertEquals(100, s.size());
        assertEquals(900L, s.first().get().ts());
        assertEquals(999L, s.last().get().ts());
    }

    @Test
    void countUnderLimitNoop() {
        TsSeriesD s = series(50);
        int removed = TsRetentionPolicy.create().maxPoints(100).apply(s);
        assertEquals(0, removed);
        assertEquals(50, s.size());
    }

    @Test
    void countZeroClears() {
        TsSeriesD s = series(10);
        int removed = TsRetentionPolicy.create().maxPoints(0).apply(s);
        assertEquals(10, removed);
        assertTrue(s.isEmpty());
    }

    @Test
    void ageKeepsWithinWindow() {
        // ts 0..999, latest 999, age 100 -> keep ts >= 899.
        TsSeriesD s = series(1_000);
        int removed = TsRetentionPolicy.create().maxAgeNs(100).apply(s);
        assertEquals(899L, s.first().get().ts());
        assertEquals(999L, s.last().get().ts());
        assertEquals(899, removed);
        assertEquals(101, s.size()); // inclusive boundary
    }

    @Test
    void ageBoundaryInclusive() {
        TsSeriesD s = series(10); // 0..9, latest 9
        // age 5 -> cutoff 4, keep ts >= 4 -> {4,5,6,7,8,9} = 6
        int removed = TsRetentionPolicy.create().maxAgeNs(5).apply(s);
        assertEquals(4, removed);
        assertEquals(6, s.size());
        assertEquals(4L, s.first().get().ts());
    }

    @Test
    void ageLargerThanSpanNoop() {
        TsSeriesD s = series(100);
        int removed = TsRetentionPolicy.create().maxAgeNs(1_000_000).apply(s);
        assertEquals(0, removed);
        assertEquals(100, s.size());
    }

    @Test
    void bytesCapsPointCount() {
        TsSeriesD s = series(1_000);
        // budget for 50 points
        int removed = TsRetentionPolicy.create()
                .maxBytes(50 * TsRetentionPolicy.BYTES_PER_POINT)
                .apply(s);
        assertEquals(50, s.size());
        assertEquals(950, removed);
        assertEquals(999L, s.last().get().ts());
    }

    @Test
    void pointCapIsTighterOfCountAndBytes() {
        TsRetentionPolicy p = TsRetentionPolicy.create()
                .maxPoints(200)
                .maxBytes(50 * TsRetentionPolicy.BYTES_PER_POINT);
        assertEquals(OptionalInt.of(50), p.pointCap());
        TsRetentionPolicy p2 = TsRetentionPolicy.create()
                .maxPoints(30)
                .maxBytes(50 * TsRetentionPolicy.BYTES_PER_POINT);
        assertEquals(OptionalInt.of(30), p2.pointCap());
        assertEquals(OptionalInt.empty(), TsRetentionPolicy.create().pointCap());
    }

    @Test
    void ageThenCountMostRestrictive() {
        TsSeriesD s = series(1_000);
        // age keeps ts>=900 (100 points), count then keeps newest 20.
        int removed = TsRetentionPolicy.create()
                .maxAgeNs(100)
                .maxPoints(20)
                .apply(s);
        assertEquals(20, s.size());
        assertEquals(980L, s.first().get().ts());
        assertEquals(999L, s.last().get().ts());
        assertEquals(980, removed);
    }

    @Test
    void emptySeriesNoop() {
        TsSeriesD s = new TsSeriesD();
        int removed = TsRetentionPolicy.create().maxPoints(10).maxAgeNs(5).apply(s);
        assertEquals(0, removed);
        assertTrue(s.isEmpty());
    }

    @Test
    void applyAllFoldsOverSeries() {
        TsSeriesD a = series(500);
        TsSeriesD b = series(300);
        TsRetentionPolicy policy = TsRetentionPolicy.create().maxPoints(100);
        int removed = policy.applyAll(List.of(a, b));
        assertEquals(400 + 200, removed);
        assertEquals(100, a.size());
        assertEquals(100, b.size());
    }

    @Test
    void worksOnGenericLongSeries() {
        TsSeries<Long> s = new TsSeries<>();
        for (long i = 0; i < 200; i++) {
            s.push(i, i * 2);
        }
        int removed = TsRetentionPolicy.create().maxPoints(64).apply(s);
        assertEquals(136, removed);
        assertEquals(64, s.size());
        assertEquals(Long.valueOf(199 * 2), s.last().get().value());
    }

    @Test
    void worksOnPrimitiveLongSeries() {
        TsSeriesL s = TsSeriesL.withCapacity(200);
        for (long i = 0; i < 200; i++) {
            s.push(i, i * 2);
        }
        int removed = TsRetentionPolicy.create().maxPoints(64).apply(s);
        assertEquals(136, removed);
        assertEquals(64, s.size());
        assertEquals(Long.valueOf(199 * 2), s.last().get().value());
        assertEquals(136L, s.first().get().ts());
    }

    @Test
    void primitiveLongAgeWindow() {
        TsSeriesL s = TsSeriesL.withCapacity(1_000);
        for (long i = 0; i < 1_000; i++) {
            s.push(i, i);
        }
        int removed = TsRetentionPolicy.create().maxAgeNs(100).apply(s);
        assertEquals(899, removed);
        assertEquals(101, s.size());
        assertEquals(899L, s.first().get().ts());
    }

    @Test
    void primitiveLongCountZeroClears() {
        TsSeriesL s = TsSeriesL.withCapacity(10);
        for (long i = 0; i < 10; i++) {
            s.push(i, i);
        }
        int removed = TsRetentionPolicy.create().maxPoints(0).apply(s);
        assertEquals(10, removed);
        assertTrue(s.isEmpty());
    }

    @Test
    void crossesChunkBoundary() {
        // > SEAL_CAP so the prune spans warm + head chunks.
        TsSeriesD s = series(150_000);
        int removed = TsRetentionPolicy.create().maxPoints(1_000).apply(s);
        assertEquals(149_000, removed);
        assertEquals(1_000, s.size());
        assertEquals(149_000L, s.first().get().ts());
        assertFalse(s.isEmpty());
    }
}
