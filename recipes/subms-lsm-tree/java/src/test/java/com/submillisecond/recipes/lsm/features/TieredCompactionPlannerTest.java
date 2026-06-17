package com.submillisecond.recipes.lsm.features;

import com.submillisecond.recipes.lsm.features.TieredRun.Entry;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

final class TieredCompactionPlannerTest {

    private static TieredRun run(long id, Object... kvs) {
        List<Entry> entries = new ArrayList<>();
        for (int i = 0; i < kvs.length; i += 2) {
            String k = (String) kvs[i];
            Object v = kvs[i + 1];
            byte[] bytes = v == null ? null : ((String) v).getBytes();
            entries.add(new Entry(k, bytes));
        }
        return new TieredRun(id, entries);
    }

    @Test
    void pickLevelFindsFullLevel() {
        TieredManifest m = new TieredManifest();
        m.push(0, run(1, "a", "1"));
        m.push(0, run(2, "b", "2"));
        m.push(0, run(3, "c", "3"));
        assertEquals(0, new TieredCompactionPlanner(3).pickLevel(m));
    }

    @Test
    void pickLevelReturnsMinusOneWhenNoLevelFull() {
        TieredManifest m = new TieredManifest();
        m.push(0, run(1, "a", "1"));
        m.push(1, run(2, "b", "2"));
        assertEquals(-1, new TieredCompactionPlanner(3).pickLevel(m));
    }

    @Test
    void mergePromotesToNextLevel() {
        TieredManifest m = new TieredManifest();
        m.push(0, run(1, "a", "1"));
        m.push(0, run(2, "b", "2"));
        m.push(0, run(3, "c", "3"));
        new TieredCompactionPlanner(3).merge(m, 0, 100);
        assertEquals(0, m.levelRunCount(0), "level 0 emptied");
        assertEquals(1, m.levelRunCount(1), "level 1 gained the merged run");
        TieredRun merged = m.levels.get(1).get(0);
        assertEquals(100, merged.id);
        List<String> keys = new ArrayList<>();
        for (Entry e : merged.entries) keys.add(e.key());
        assertEquals(Arrays.asList("a", "b", "c"), keys);
    }

    @Test
    void newerRunWinsOnKeyCollision() {
        TieredManifest m = new TieredManifest();
        m.push(0, run(1, "k", "old"));
        m.push(0, run(2, "k", "new"));
        new TieredCompactionPlanner(2).merge(m, 0, 50);
        TieredRun merged = m.levels.get(1).get(0);
        assertEquals(1, merged.entries.size());
        assertArrayEquals("new".getBytes(), merged.entries.get(0).value());
    }

    @Test
    void tombstoneIsPreservedInMerge() {
        TieredManifest m = new TieredManifest();
        m.push(0, run(1, "k", "v"));
        m.push(0, run(2, "k", null));
        new TieredCompactionPlanner(2).merge(m, 0, 50);
        TieredRun merged = m.levels.get(1).get(0);
        assertEquals(1, merged.entries.size());
        assertNull(merged.entries.get(0).value(), "tombstone wins");
    }

    @Test
    void runsPerLevelFloorIsTwo() {
        assertEquals(2, new TieredCompactionPlanner(0).runsPerLevel());
        assertEquals(2, new TieredCompactionPlanner(1).runsPerLevel());
    }

    @Test
    void mergeHandlesNonOverlappingKeys() {
        TieredManifest m = new TieredManifest();
        m.push(0, run(1, "a", "1", "c", "3"));
        m.push(0, run(2, "b", "2", "d", "4"));
        new TieredCompactionPlanner(2).merge(m, 0, 99);
        TieredRun merged = m.levels.get(1).get(0);
        List<String> keys = new ArrayList<>();
        for (Entry e : merged.entries) keys.add(e.key());
        assertEquals(Arrays.asList("a", "b", "c", "d"), keys);
    }

    @Test
    void cascadingCompactionViaRepeatedPickAndMerge() {
        TieredManifest m = new TieredManifest();
        for (int i = 0; i < 4; i++) m.push(0, run(i, "k" + i, "v"));
        TieredCompactionPlanner p = new TieredCompactionPlanner(4);
        int lvl = p.pickLevel(m);
        p.merge(m, lvl, 10);
        assertEquals(-1, p.pickLevel(m), "single merged run does not trigger again");
        assertEquals(1, m.levelRunCount(1));
        assertEquals(4, m.levels.get(1).get(0).entries.size());
    }
}
