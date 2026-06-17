package com.submillisecond.recipes.lsm.features;

import com.submillisecond.recipes.lsm.features.TieredRun.Entry;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

final class LeveledCompactionPlannerTest {

    private static LeveledRun run(long id, Object... kvs) {
        List<Entry> entries = new ArrayList<>();
        for (int i = 0; i < kvs.length; i += 2) {
            String k = (String) kvs[i];
            Object v = kvs[i + 1];
            byte[] bytes = v == null ? null : ((String) v).getBytes();
            entries.add(new Entry(k, bytes));
        }
        return new LeveledRun(id, entries);
    }

    @Test
    void levelBudgetGrowsByFanout() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(100, 10, 4);
        assertEquals(100, p.levelBudget(1));
        assertEquals(1_000, p.levelBudget(2));
        assertEquals(10_000, p.levelBudget(3));
    }

    @Test
    void pickLevelFiresOnL0RunLimit() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(10_000, 10, 2);
        LeveledManifest m = new LeveledManifest();
        m.push(0, run(1, "a", "v"));
        m.push(0, run(2, "b", "v"));
        assertEquals(0, p.pickLevel(m));
    }

    @Test
    void pickLevelFiresWhenLevelOverBudget() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(10, 10, 10);
        LeveledManifest m = new LeveledManifest();
        byte[] big = new byte[100];
        for (int i = 0; i < big.length; i++) big[i] = (byte) 'x';
        List<Entry> entries = new ArrayList<>();
        entries.add(new Entry("k", big));
        m.push(1, new LeveledRun(1, entries));
        assertEquals(1, p.pickLevel(m));
    }

    @Test
    void compactL0IntoL1ProducesNonOverlapping() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(1_000_000, 10, 2);
        LeveledManifest m = new LeveledManifest();
        m.push(0, run(1, "a", "1", "c", "3"));
        m.push(0, run(2, "b", "2", "d", "4"));
        m.push(1, run(3, "a", "old", "e", "5"));
        p.compact(m, 0, 100);
        assertTrue(m.levelIsNonOverlapping(1), "L1 must be key-disjoint after compaction");
        LeveledRun merged = m.levels.get(1).get(0);
        Map<String, byte[]> map = new HashMap<>();
        for (Entry e : merged.entries) {
            if (e.value() != null) map.put(e.key(), e.value());
        }
        assertArrayEquals("1".getBytes(), map.get("a"), "L0 'a=1' shadows L1 'a=old'");
        assertArrayEquals("5".getBytes(), map.get("e"), "L1 'e=5' carried through");
        assertEquals(0, m.levelRunCount(0));
    }

    @Test
    void compactSingleL1RunIntoL2() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(1_000_000, 10, 10);
        LeveledManifest m = new LeveledManifest();
        m.push(1, run(1, "a", "1"));
        m.push(2, run(2, "a", "old", "c", "3"));
        p.compact(m, 1, 50);
        assertEquals(0, m.levelRunCount(1));
        assertEquals(1, m.levelRunCount(2));
        LeveledRun merged = m.levels.get(2).get(0);
        Map<String, byte[]> map = new HashMap<>();
        for (Entry e : merged.entries) {
            if (e.value() != null) map.put(e.key(), e.value());
        }
        assertArrayEquals("1".getBytes(), map.get("a"), "L1 wins over L2");
        assertTrue(map.containsKey("c"));
    }

    @Test
    void compactPreservesNonOverlappingL1Runs() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(1_000_000, 10, 10);
        LeveledManifest m = new LeveledManifest();
        m.push(0, run(1, "a", "1"));
        m.push(1, run(2, "a", "old"));
        m.push(1, run(3, "z", "zed")); // disjoint - must survive
        p.compact(m, 0, 100);
        assertTrue(m.levelIsNonOverlapping(1));
        boolean zSurvives = false;
        for (LeveledRun r : m.levels.get(1)) {
            for (Entry e : r.entries) if (e.key().equals("z")) zSurvives = true;
        }
        assertTrue(zSurvives, "disjoint L1 run must not be dragged into the merge");
    }

    @Test
    void tombstoneCarriedThroughLevels() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(1_000_000, 10, 2);
        LeveledManifest m = new LeveledManifest();
        m.push(0, run(1, "k", "v"));
        m.push(0, run(2, "k", null));
        p.compact(m, 0, 100);
        LeveledRun merged = m.levels.get(1).get(0);
        assertEquals(1, merged.entries.size());
        assertNull(merged.entries.get(0).value(), "tombstone shadowed the put");
    }

    @Test
    void pickLevelReturnsMinusOneWhenNoLevelFull() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(10_000, 10, 5);
        LeveledManifest m = new LeveledManifest();
        m.push(0, run(1, "a", "v"));
        m.push(1, run(2, "b", "v"));
        assertEquals(-1, p.pickLevel(m));
    }

    @Test
    void emptyCompactIsNoop() {
        LeveledCompactionPlanner p = new LeveledCompactionPlanner(10_000, 10, 5);
        LeveledManifest m = new LeveledManifest();
        m.levels.add(new ArrayList<>());
        p.compact(m, 0, 100);
        assertEquals(0, m.totalRunCount());
    }

    @Test
    void overlapsDetectedBetweenRuns() {
        LeveledRun a = run(1, "a", "x", "c", "y");
        LeveledRun b = run(2, "b", "z");
        LeveledManifest m = new LeveledManifest();
        m.push(1, a);
        m.push(1, b);
        // levelIsNonOverlapping should see the overlap (b is inside a..c).
        assertFalse(m.levelIsNonOverlapping(1));
    }
}
