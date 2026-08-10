package com.submillisecond.recipes.treap;

import com.submillisecond.recipes.treap.features.PersistentTreap;
import com.submillisecond.recipes.treap.features.SplittableTreap;
import com.submillisecond.recipes.treap.features.TreapSnapshot;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;
import java.util.Map;

/**
 * Sample app: a bid-side depth book built on {@code subms-treap}.
 *
 * <p>A fixed tape of order events is applied to a price-level index, then the
 * book is read the way a trading system reads it - top of book first, a band
 * around the touch, a sweep off the top - and finally rebuilt from a sorted
 * snapshot. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.treap.SampleApp}
 *
 * <p>Keys are price levels in integer ticks, values are the resting quantity
 * at that level. Everything is seeded and the tape is fixed, so the output is
 * byte-identical on every run.
 *
 * <ul>
 *   <li>base             - apply a tape, read the ladder, sweep the touch, restore
 *   <li>range scan       - resting depth within a price band, ascending
 *   <li>persistent       - version the book so a prior state stays queryable
 *   <li>merge-split      - partition the ladder at the touch and stitch it back
 *   <li>concurrent-reads - publish a frozen book to reader threads under writer churn
 * </ul>
 */
public final class SampleApp {

    static final long SEED = 0xB1DL;

    /** One line of the order tape. */
    enum Kind { POST, AMEND, CANCEL }

    record Event(Kind kind, int price, long qty) { }

    /** A fixed tape. Deterministic input is the point: the printed report is
     *  reproducible, which a page quoting that output depends on. */
    static final List<Event> TAPE = List.of(
            new Event(Kind.POST, 9998, 1_000),
            new Event(Kind.POST, 10_000, 500),
            new Event(Kind.POST, 9999, 250),
            new Event(Kind.POST, 10_001, 100),
            new Event(Kind.POST, 9997, 750),
            new Event(Kind.POST, 10_002, 400),
            new Event(Kind.POST, 9995, 300),
            new Event(Kind.POST, 9993, 150),
            new Event(Kind.POST, 9996, 600),
            new Event(Kind.AMEND, 10_000, 150),
            new Event(Kind.AMEND, 10_001, 800),
            new Event(Kind.CANCEL, 9997, 0),
            new Event(Kind.POST, 9994, 220),
            new Event(Kind.AMEND, 9993, -50));

    public static void main(String[] args) throws InterruptedException {
        System.out.println("== bid-side depth book ==");
        Treap<Integer, Long> book = buildBook();
        System.out.println("  applied " + TAPE.size() + " events -> " + book.size() + " levels");
        report(book);
        sweepTheTouch(book);
        restoreFromSnapshot(book);

        bandDepth();
        versionedBook();
        partitionLadder();
        publishedSnapshot();
    }

    /** Apply the tape. A post inserts or replaces a level, an amend adjusts the
     *  resting quantity in place through {@code compute} (no re-descent, no
     *  priority redraw), a cancel removes the level. */
    static Treap<Integer, Long> buildBook() {
        Treap<Integer, Long> book = new Treap<>(SEED);
        for (Event e : TAPE) {
            switch (e.kind()) {
                case POST -> book.insert(e.price(), e.qty());
                case AMEND -> book.compute(e.price(), q -> q + e.qty());
                case CANCEL -> book.remove(e.price());
            }
        }
        if (book.size() != 9) throw new AssertionError("nine resting levels");
        if (!Long.valueOf(650L).equals(book.get(10_000))) {
            throw new AssertionError("amend applied in place");
        }
        if (book.containsKey(9997)) throw new AssertionError("cancelled level is gone");
        return book;
    }

    /** Read the book the way a trader does: best price first, then the touch
     *  and its neighbours. {@code descendingIterator} walks the ladder high to
     *  low; {@code floor} and {@code predecessor} answer "what is at or below
     *  this price" without a scan. */
    static void report(Treap<Integer, Long> book) {
        Map.Entry<Integer, Long> best = book.last();
        System.out.println("  best bid " + best.getKey() + " x " + best.getValue()
                + " | height " + book.height() + " | " + book.size() + " levels");

        System.out.println("  top 5, best first:");
        Iterator<Map.Entry<Integer, Long>> down = book.descendingIterator();
        for (int i = 0; i < 5 && down.hasNext(); i++) {
            Map.Entry<Integer, Long> e = down.next();
            System.out.printf("    %d  %5d%n", e.getKey(), e.getValue());
        }

        int inside = book.predecessor(best.getKey()).getKey();
        System.out.println("  next level down: " + inside);
        if (inside != 10_001) throw new AssertionError("inside level");

        // A price that is not a resting level still answers, which is the whole
        // reason for an ordered index over a hash map.
        int probe = 9_990;
        System.out.println("  probe " + probe + ": floor " + book.floor(probe)
                + ", ceiling " + book.ceiling(probe).getKey());
        if (book.floor(probe) != null) throw new AssertionError("nothing rests below the probe");
        if (book.ceiling(probe).getKey() != 9993) throw new AssertionError("ceiling level");
    }

    /** Sweep an aggressive sell through the bid side. {@code popLast} takes the
     *  best level in expected O(log n) and hands back both key and value, so the
     *  fill loop never re-descends to find the next price. */
    static void sweepTheTouch(Treap<Integer, Long> book) {
        long toFill = 1_200L;
        List<String> fills = new ArrayList<>();
        while (toFill > 0) {
            Map.Entry<Integer, Long> level = book.popLast();
            if (level == null) break;
            long take = Math.min(level.getValue(), toFill);
            toFill -= take;
            fills.add(level.getKey() + "x" + take);
            if (level.getValue() > take) {
                book.insert(level.getKey(), level.getValue() - take); // partial fill
            }
        }
        System.out.println("  sweep 1200 lots -> " + fills);
        if (!fills.equals(List.of("10002x400", "10001x800"))) throw new AssertionError("fills");
        if (book.size() != 8) throw new AssertionError("levels after the sweep");
        if (book.last().getKey() != 10_001) throw new AssertionError("partial fill left the level");
    }

    /** End-of-day restore. {@code collectEntriesInOrder} gives a sorted
     *  snapshot; {@code fromSorted} rebuilds in O(n) instead of paying n
     *  rotating inserts. */
    static void restoreFromSnapshot(Treap<Integer, Long> book) {
        List<Map.Entry<Integer, Long>> snapshot = book.collectEntriesInOrder();
        Treap<Integer, Long> restored = Treap.fromSorted(SEED, snapshot);
        System.out.println("  restored " + restored.size()
                + " levels from a sorted snapshot, height " + restored.height());
        if (!restored.collectEntriesInOrder().equals(snapshot)) {
            throw new AssertionError("snapshot round trip");
        }

        // Unsorted input is rejected rather than silently reordered.
        try {
            Treap.fromSorted(SEED, List.of(Map.entry(2, 1L), Map.entry(1, 1L)));
            throw new AssertionError("strictly-ascending precondition enforced");
        } catch (IllegalArgumentException expected) {
            // the precondition held
        }
    }

    /** Range scan: sum resting depth in a price band without materialising the
     *  whole ladder. Descends to the low bound then walks only the window. Each
     *  bound is independently inclusive or exclusive. */
    static void bandDepth() {
        System.out.println("\n== range scan: depth in a price band ==");
        Treap<Integer, Long> book = buildBook();
        int lo = 9_996, hi = 10_000;
        long depth = 0;
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Long> e : book.range(lo, true, hi, true)) {
            keys.add(e.getKey());
            depth += e.getValue();
        }
        System.out.println("  [" + lo + ", " + hi + "] -> " + keys.size()
                + " levels, " + depth + " lots");
        if (!keys.equals(List.of(9_996, 9_998, 9_999, 10_000))) throw new AssertionError("band keys");
        if (depth != 2_500L) throw new AssertionError("band depth");

        // Exclusive upper bound drops the touch itself.
        long inside = 0;
        for (Map.Entry<Integer, Long> e : book.range(lo, true, hi, false)) {
            inside += e.getValue();
        }
        System.out.println("  same band, exclusive of " + hi + ": " + inside + " lots");
        if (inside != 1_850L) throw new AssertionError("exclusive band depth");
    }

    /** persistent: version the book so a prior state stays queryable. Each
     *  insert / remove returns a NEW book and leaves the receiver untouched -
     *  the shape a risk what-if branch or an audit trail wants. */
    static void versionedBook() {
        System.out.println("\n== persistent: versioned book ==");
        PersistentTreap<Integer, Long> open = new PersistentTreap<Integer, Long>(SEED)
                .insert(9_999, 250L).insert(10_000, 500L).insert(10_001, 100L);

        // Branch: what does the book look like if the 9999 level fills?
        PersistentTreap<Integer, Long> afterFill = open.remove(9_999);
        System.out.println("  open: " + open.size() + " levels, depth@9999 " + open.get(9_999));
        System.out.println("  after fill: " + afterFill.size()
                + " levels, depth@9999 " + afterFill.get(9_999));
        if (!Long.valueOf(250L).equals(open.get(9_999))) throw new AssertionError("prior version intact");
        if (afterFill.get(9_999) != null) throw new AssertionError("fill applied");
        if (open.size() != 3 || afterFill.size() != 2) throw new AssertionError("version sizes");
    }

    /** merge-split: partition the ladder at the touch in expected O(log N),
     *  then stitch it back. This is the treap's distinguishing operation - a
     *  red-black tree has no cheap equivalent. merge requires every key on the
     *  left to be strictly less than every key on the right. */
    static void partitionLadder() {
        System.out.println("\n== merge-split: partition at the touch ==");
        SplittableTreap<Integer, Long> book = new SplittableTreap<>(SEED);
        book.insert(9_996, 600L);
        book.insert(9_998, 1_000L);
        book.insert(9_999, 250L);
        book.insert(10_000, 650L);
        book.insert(10_001, 900L);
        book.insert(10_002, 400L);

        // Everything strictly below 10000 is the resting book; 10000 and above
        // is the band a marketable order would clear against.
        SplittableTreap.Split<Integer, Long> parts = book.split(10_000);
        System.out.println("  below 10000: " + parts.left.size()
                + " levels | 10000 and up: " + parts.right.size() + " levels");
        if (parts.left.size() != 3 || parts.right.size() != 3) throw new AssertionError("split");
        if (parts.right.collectInOrder().get(0).getKey() != 10_000) {
            throw new AssertionError("pivot lands on the right");
        }

        SplittableTreap<Integer, Long> rejoined = SplittableTreap.merge(parts.left, parts.right);
        List<Integer> keys = new ArrayList<>();
        for (Map.Entry<Integer, Long> e : rejoined.collectInOrder()) keys.add(e.getKey());
        System.out.println("  rejoined: " + keys);
        if (!keys.equals(List.of(9_996, 9_998, 9_999, 10_000, 10_001, 10_002))) {
            throw new AssertionError("merge round-trips the ladder");
        }
    }

    /** concurrent-reads: freeze the book into a shared snapshot and fan it
     *  out to reader threads (market-data / risk consumers) while the writer
     *  keeps applying updates. Every reader sees a stable point-in-time book. */
    static void publishedSnapshot() throws InterruptedException {
        System.out.println("\n== concurrent-reads: published book snapshot ==");
        Treap<Integer, Long> book = new Treap<>(SEED);
        for (int px = 9_990; px < 10_010; px++) book.insert(px, px * 10L);
        TreapSnapshot<Integer, Long> snap = TreapSnapshot.fromTreap(book);

        int[] counts = new int[4];
        List<Thread> readers = new ArrayList<>();
        for (int i = 0; i < 4; i++) {
            final int slot = i;
            Thread t = new Thread(() -> counts[slot] = snap.range(9_995, 10_004).size());
            readers.add(t);
            t.start();
        }

        // Writer churn after the snapshot: readers must not observe it.
        book.insert(12_345, 1L);
        book.remove(9_990);

        for (Thread t : readers) t.join();
        for (int c : counts) {
            if (c != 10) throw new AssertionError("reader sees the frozen 10-level band");
        }
        System.out.println("  4 readers each counted 10 levels in [9995, 10004]");
        if (snap.get(12_345) != null) throw new AssertionError("snapshot isolated from later writes");
        if (snap.size() != 20) throw new AssertionError("snapshot size");
    }
}
