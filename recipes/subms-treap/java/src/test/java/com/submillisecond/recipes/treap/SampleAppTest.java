package com.submillisecond.recipes.treap;

import com.submillisecond.recipes.treap.features.PersistentTreap;
import com.submillisecond.recipes.treap.features.RangeQuery;
import com.submillisecond.recipes.treap.features.SplittableTreap;
import com.submillisecond.recipes.treap.features.TreapSnapshot;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNull;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        Treap<Integer, String> t = new Treap<>(42L);
        t.insert(3, "three");
        t.insert(1, "one");
        t.insert(2, "two");
        assertEquals("two", t.get(2));   // BST lookup by key
        assertEquals(3, t.size());
        assertEquals("one", t.remove(1)); // remove returns the prior value
        assertEquals(2, t.size());
        // quickstart:end
    }

    @Test
    void bidLadderScenario() {
        Treap<Integer, Long> book = new Treap<>(0xB1DL);
        book.insert(9998, 1_000L);
        book.insert(10_000, 500L);
        book.insert(9999, 250L);
        book.insert(10_001, 100L);
        book.insert(9997, 750L);
        assertEquals(5, book.size());

        assertEquals(500L, book.insert(10_000, 650L), "amend replaces resting quantity");
        assertEquals(650L, book.get(10_000));

        assertEquals(750L, book.remove(9997), "cancel returns the removed quantity");
        assertNull(book.get(9997));

        assertEquals(List.of(9998, 9999, 10_000, 10_001), book.collectInOrder());
    }

    @Test
    void bandDepthIsWindowedAndSorted() {
        Treap<Integer, Long> book = new Treap<>(0xB1DL);
        book.insert(9998, 1_000L);
        book.insert(9999, 250L);
        book.insert(10_000, 650L);
        book.insert(10_001, 100L);
        book.insert(10_002, 900L);

        List<Integer> keys = new ArrayList<>();
        long depth = 0;
        for (Map.Entry<Integer, Long> e : RangeQuery.of(book, 9999, true, 10_001, true)) {
            keys.add(e.getKey());
            depth += e.getValue();
        }
        assertEquals(List.of(9999, 10_000, 10_001), keys);
        assertEquals(1_000L, depth);
    }

    @Test
    void versionedBookKeepsPriorState() {
        PersistentTreap<Integer, Long> v0 = new PersistentTreap<>(0xB1DL);
        PersistentTreap<Integer, Long> v1 = v0.insert(10_000, 500L).insert(9999, 250L);
        PersistentTreap<Integer, Long> v2 = v1.remove(9999);
        assertEquals(250L, v1.get(9999), "old version untouched by later fill");
        assertNull(v2.get(9999));
        assertEquals(2, v1.size());
        assertEquals(1, v2.size());
    }

    @Test
    void splitThenMergeRoundTrips() {
        SplittableTreap<Integer, Long> book = new SplittableTreap<>(0xB1DL);
        book.insert(9998, 1_000L);
        book.insert(9999, 250L);
        book.insert(10_000, 650L);
        book.insert(10_001, 100L);

        SplittableTreap.Split<Integer, Long> parts = book.split(10_000);
        assertEquals(2, parts.left.size());
        assertEquals(2, parts.right.size());

        SplittableTreap<Integer, Long> rejoined = SplittableTreap.merge(parts.left, parts.right);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Long> e : rejoined.collectInOrder()) keys.add(e.getKey());
        assertEquals(List.of(9998, 9999, 10_000, 10_001), keys);
    }

    @Test
    void publishedSnapshotIsolatedFromWrites() throws InterruptedException {
        Treap<Integer, Long> book = new Treap<>(0xB1DL);
        for (int px = 9990; px < 10_010; px++) book.insert(px, px * 10L);
        TreapSnapshot<Integer, Long> snap = TreapSnapshot.fromTreap(book);

        book.insert(12_345, 1L);
        book.remove(9990);

        assertEquals(10, snap.range(9995, 10_004).size(), "frozen band count");
        assertNull(snap.get(12_345), "snapshot isolated from later writes");
        assertEquals(99_900L, snap.get(9990), "removed key still in snapshot");
        assertEquals(20, snap.size());
    }
}
