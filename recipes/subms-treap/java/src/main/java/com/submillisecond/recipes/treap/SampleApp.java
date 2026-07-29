package com.submillisecond.recipes.treap;

import com.submillisecond.recipes.treap.features.PersistentTreap;
import com.submillisecond.recipes.treap.features.RangeQuery;
import com.submillisecond.recipes.treap.features.SplittableTreap;
import com.submillisecond.recipes.treap.features.TreapSnapshot;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Sample app: a tour of {@code subms-treap} as a limit-order-book price-level
 * index, base API first, then each optional feature class in the
 * {@code features} sub-package. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.treap.SampleApp}
 *
 * <p>Keys are price levels in integer ticks, values are the resting quantity
 * at that level. The ordered map keeps the ladder sorted by price.
 *
 * <ul>
 *   <li>base             - a bid ladder: post, amend, read, cancel a level
 *   <li>range-query      - resting depth within a price band, ascending
 *   <li>persistent       - snapshot the book, keep prior versions queryable
 *   <li>merge-split      - partition the ladder at a price and stitch it back
 *   <li>concurrent-reads - publish a frozen book to reader threads under writer churn
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws InterruptedException {
        baseBidLadder();
        bandDepth();
        versionedBook();
        partitionLadder();
        publishedSnapshot();
    }

    /** Base API: a single-sided bid ladder keyed by price tick, valued by
     *  the resting quantity at that level. */
    static void baseBidLadder() {
        System.out.println("== base: bid ladder ==");
        Treap<Integer, Long> book = new Treap<>(0xB1DL);
        book.insert(9998, 1_000L);
        book.insert(10_000, 500L);
        book.insert(9999, 250L);
        book.insert(10_001, 100L);
        book.insert(9997, 750L);
        System.out.println("  posted " + book.size() + " price levels");
        if (book.size() != 5) throw new AssertionError("five levels posted");

        // A fresh post at an existing level replaces the resting quantity.
        Long prev = book.insert(10_000, 650L);
        System.out.println("  amend 10000: was " + prev + ", now " + book.get(10_000));
        if (!Long.valueOf(500L).equals(prev)) throw new AssertionError("amend returns prior qty");
        if (!Long.valueOf(650L).equals(book.get(10_000))) throw new AssertionError("amend applied");

        Long cancelled = book.remove(9997);
        System.out.println("  cancel 9997 -> " + cancelled);
        if (!Long.valueOf(750L).equals(cancelled)) throw new AssertionError("cancel returns qty");
        if (book.get(9997) != null) throw new AssertionError("cancelled level is gone");

        List<Integer> ladder = book.collectInOrder();
        System.out.println("  ladder (low->high): " + ladder);
        if (!ladder.equals(List.of(9998, 9999, 10_000, 10_001))) {
            throw new AssertionError("ladder sorted by price");
        }
    }

    /** range-query: sum resting depth in a price band without materialising
     *  the whole ladder. Descends to the low bound then walks the window. */
    static void bandDepth() {
        System.out.println("\n== range-query: depth in a price band ==");
        Treap<Integer, Long> book = new Treap<>(0xB1DL);
        book.insert(9998, 1_000L);
        book.insert(9999, 250L);
        book.insert(10_000, 650L);
        book.insert(10_001, 100L);
        book.insert(10_002, 900L);

        RangeQuery<Integer, Long> band = RangeQuery.of(book, 9999, true, 10_001, true);
        long depth = 0;
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Long> e : band) {
            keys.add(e.getKey());
            depth += e.getValue();
        }
        System.out.println("  [9999, 10001] -> " + keys + ", total depth " + depth);
        if (!keys.equals(List.of(9999, 10_000, 10_001))) throw new AssertionError("windowed keys");
        if (depth != 1_000L) throw new AssertionError("band depth");
    }

    /** persistent: snapshot the book cheaply and keep prior versions
     *  queryable. Each insert / remove returns a NEW book. */
    static void versionedBook() {
        System.out.println("\n== persistent: versioned book ==");
        PersistentTreap<Integer, Long> v0 = new PersistentTreap<>(0xB1DL);
        PersistentTreap<Integer, Long> v1 = v0.insert(10_000, 500L).insert(9999, 250L);
        PersistentTreap<Integer, Long> v2 = v1.remove(9999); // 9999 fully filled
        System.out.println("  v1 depth@9999 " + v1.get(9999) + ", v2 depth@9999 " + v2.get(9999));
        if (!Long.valueOf(250L).equals(v1.get(9999))) throw new AssertionError("v1 untouched");
        if (v2.get(9999) != null) throw new AssertionError("v2 has the fill applied");
        if (v1.size() != 2 || v2.size() != 1) throw new AssertionError("version sizes");
    }

    /** merge-split: partition the ladder at a price, then stitch it back. */
    static void partitionLadder() {
        System.out.println("\n== merge-split: partition the ladder ==");
        SplittableTreap<Integer, Long> book = new SplittableTreap<>(0xB1DL);
        book.insert(9998, 1_000L);
        book.insert(9999, 250L);
        book.insert(10_000, 650L);
        book.insert(10_001, 100L);

        SplittableTreap.Split<Integer, Long> parts = book.split(10_000);
        System.out.println("  below 10000: " + parts.left.size()
                + " levels, 10000+: " + parts.right.size() + " levels");
        if (parts.left.size() != 2 || parts.right.size() != 2) {
            throw new AssertionError("split at the pivot");
        }
        SplittableTreap<Integer, Long> rejoined = SplittableTreap.merge(parts.left, parts.right);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Long> e : rejoined.collectInOrder()) keys.add(e.getKey());
        System.out.println("  rejoined: " + keys);
        if (!keys.equals(List.of(9998, 9999, 10_000, 10_001))) {
            throw new AssertionError("merge round-trips the ladder");
        }
    }

    /** concurrent-reads: freeze the book into a shared snapshot and fan it
     *  out to reader threads while the writer keeps applying updates. */
    static void publishedSnapshot() throws InterruptedException {
        System.out.println("\n== concurrent-reads: published book snapshot ==");
        Treap<Integer, Long> book = new Treap<>(0xB1DL);
        for (int px = 9990; px < 10_010; px++) book.insert(px, px * 10L);
        TreapSnapshot<Integer, Long> snap = TreapSnapshot.fromTreap(book);

        int[] counts = new int[4];
        List<Thread> readers = new ArrayList<>();
        for (int i = 0; i < 4; i++) {
            final int slot = i;
            Thread t = new Thread(() -> counts[slot] = snap.range(9995, 10_004).size());
            readers.add(t);
            t.start();
        }

        // Writer churn after the snapshot: readers must not observe it.
        book.insert(12_345, 1L);
        book.remove(9990);

        for (Thread t : readers) t.join();
        for (int c : counts) {
            if (c != 10) throw new AssertionError("reader sees the frozen 10-level band");
        }
        System.out.println("  4 readers each counted 10 levels in [9995, 10004]");
        if (snap.get(12_345) != null) throw new AssertionError("snapshot isolated from later writes");
        if (snap.size() != 20) throw new AssertionError("snapshot size");
    }
}
