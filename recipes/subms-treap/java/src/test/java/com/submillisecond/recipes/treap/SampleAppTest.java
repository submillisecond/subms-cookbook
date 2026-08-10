package com.submillisecond.recipes.treap;

import com.submillisecond.recipes.treap.features.PersistentTreap;
import com.submillisecond.recipes.treap.features.SplittableTreap;
import com.submillisecond.recipes.treap.features.TreapSnapshot;
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. The
 *  sample app carries its own assertions, so these drive its real methods
 *  rather than a copy of them. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        Treap<Integer, String> t = new Treap<>(42L);
        t.insert(3, "three");
        t.insert(1, "one");
        t.insert(2, "two");
        assertEquals("two", t.get(2));    // BST lookup by key
        assertEquals(3, t.size());
        assertEquals("one", t.remove(1)); // remove returns the prior value
        assertEquals(2, t.size());

        // Ordered navigation, no optional feature needed.
        assertEquals(2, t.first().getKey());
        assertEquals(3, t.ceiling(3).getKey());
        assertEquals(2, t.predecessor(3).getKey());
        assertEquals(List.of(2, 3), t.collectInOrder());
        // quickstart:end
    }

    @Test
    void sampleAppRunsEndToEnd() throws InterruptedException {
        SampleApp.main(new String[0]);
    }

    @Test
    void tapeProducesTheDocumentedBook() {
        Treap<Integer, Long> book = SampleApp.buildBook();
        assertEquals(9, book.size());
        assertEquals(650L, book.get(10_000), "two amends applied in place");
        assertEquals(100L, book.get(9993), "negative amend applied");
        assertNull(book.get(9997), "cancelled level is gone");
        assertEquals(10_002, book.last().getKey());
        assertEquals(9993, book.first().getKey());
    }

    @Test
    void reportAndSweepHoldTheirAssertions() {
        Treap<Integer, Long> book = SampleApp.buildBook();
        SampleApp.report(book);
        SampleApp.sweepTheTouch(book);
        assertEquals(8, book.size());
        assertEquals(10_001, book.last().getKey());
        assertEquals(100L, book.get(10_001), "partial fill left 100 lots resting");
    }

    @Test
    void restoreRebuildsFromASortedSnapshot() {
        Treap<Integer, Long> book = SampleApp.buildBook();
        List<Map.Entry<Integer, Long>> snapshot = book.collectEntriesInOrder();
        Treap<Integer, Long> restored = Treap.fromSorted(SampleApp.SEED, snapshot);
        assertEquals(snapshot, restored.collectEntriesInOrder());
        assertTrue(restored.height() <= book.height() + 4, "bulk build stays shallow");
        assertThrows(IllegalArgumentException.class,
                () -> Treap.fromSorted(SampleApp.SEED, List.of(Map.entry(2, 1L), Map.entry(1, 1L))));
    }

    @Test
    void bandDepthIsWindowedAndSorted() {
        Treap<Integer, Long> book = SampleApp.buildBook();
        List<Integer> keys = new ArrayList<>();
        long depth = 0;
        for (Map.Entry<Integer, Long> e : book.range(9_996, true, 10_000, true)) {
            keys.add(e.getKey());
            depth += e.getValue();
        }
        assertEquals(List.of(9_996, 9_998, 9_999, 10_000), keys);
        assertEquals(2_500L, depth);

        long inside = 0;
        for (Map.Entry<Integer, Long> e : book.range(9_996, true, 10_000, false)) {
            inside += e.getValue();
        }
        assertEquals(1_850L, inside, "exclusive upper bound drops the touch");
    }

    @Test
    void versionedBookKeepsPriorState() {
        PersistentTreap<Integer, Long> open = new PersistentTreap<Integer, Long>(SampleApp.SEED)
                .insert(9_999, 250L).insert(10_000, 500L).insert(10_001, 100L);
        PersistentTreap<Integer, Long> afterFill = open.remove(9_999);
        assertEquals(250L, open.get(9_999), "old version untouched by later fill");
        assertNull(afterFill.get(9_999));
        assertEquals(3, open.size());
        assertEquals(2, afterFill.size());
    }

    @Test
    void splitThenMergeRoundTrips() {
        SplittableTreap<Integer, Long> book = new SplittableTreap<>(SampleApp.SEED);
        book.insert(9_996, 600L);
        book.insert(9_998, 1_000L);
        book.insert(9_999, 250L);
        book.insert(10_000, 650L);
        book.insert(10_001, 900L);
        book.insert(10_002, 400L);

        SplittableTreap.Split<Integer, Long> parts = book.split(10_000);
        assertEquals(3, parts.left.size());
        assertEquals(3, parts.right.size());
        assertEquals(10_000, parts.right.collectInOrder().get(0).getKey(), "pivot lands right");

        SplittableTreap<Integer, Long> rejoined = SplittableTreap.merge(parts.left, parts.right);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Long> e : rejoined.collectInOrder()) keys.add(e.getKey());
        assertEquals(List.of(9_996, 9_998, 9_999, 10_000, 10_001, 10_002), keys);
    }

    @Test
    void publishedSnapshotIsolatedFromWrites() {
        Treap<Integer, Long> book = new Treap<>(SampleApp.SEED);
        for (int px = 9990; px < 10_010; px++) book.insert(px, px * 10L);
        TreapSnapshot<Integer, Long> snap = TreapSnapshot.fromTreap(book);

        book.insert(12_345, 1L);
        book.remove(9990);

        assertEquals(10, snap.range(9995, 10_004).size(), "frozen band count");
        assertNull(snap.get(12_345), "snapshot isolated from later writes");
        assertEquals(99_900L, snap.get(9990), "removed key still in snapshot");
        assertEquals(20, snap.size());
        assertNotNull(snap.entries());
    }
}
