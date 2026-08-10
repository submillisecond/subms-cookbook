package com.submillisecond.recipes.mergeiter;

import com.submillisecond.recipes.mergeiter.features.DedupEntry;
import com.submillisecond.recipes.mergeiter.features.DedupMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PriorityEntry;
import com.submillisecond.recipes.mergeiter.features.PriorityMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PrioritySource;
import com.submillisecond.recipes.mergeiter.features.ReverseMergeIterator;
import com.submillisecond.recipes.mergeiter.features.SeekableMergeIterator;
import com.submillisecond.recipes.mergeiter.features.TombstoneEntry;
import com.submillisecond.recipes.mergeiter.features.TombstoneMergeIterator;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Sample app: a miniature market-data store read entirely through merge
 * iterators. One session of data is declared up front; every section below is
 * a different query against it. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.mergeiter.SampleApp}
 *
 * <p>The store holds three things, the way an LSM-backed tick store does: a
 * trade tape per venue, a bid ladder per venue, and reference and last-price
 * rows spread across levels (oldest flushed level first, live memtable last).
 *
 * <ul>
 *   <li>base       - consolidate the per-venue tapes into one chronological tape
 *   <li>seek-to    - read one half-open session window out of that tape
 *   <li>reverse    - walk the consolidated bid ladder down from the top of book
 *   <li>tombstones - resolve the reference rows, honouring a delisting
 *   <li>dedup      - collapse the last-price rows to the freshest per symbol
 *   <li>priority   - the same rows, with the memtable stated as authoritative
 * </ul>
 */
public final class SampleApp {

    /**
     * Trade timestamps in ns since the session epoch, one ascending tape per
     * venue. 9_300 is the open and 9_800 the close.
     */
    private static final List<List<Integer>> VENUE_TAPES = List.of(
        List.of(8_000, 9_100, 9_400, 9_800),
        List.of(8_500, 9_300, 9_600),
        List.of(9_050, 9_450, 9_900));

    /**
     * Bid price levels in ticks, one descending ladder per venue - the order a
     * depth feed already publishes them in.
     */
    private static final List<List<Integer>> BID_LADDERS = List.of(
        List.of(10_120, 10_105, 10_101, 10_095),
        List.of(10_118, 10_110, 10_099));

    /**
     * Instrument reference rows, oldest level first, sorted by symbol within a
     * level. A null status is a tombstone: the delisting written to the newest
     * level.
     */
    private static final List<List<TombstoneEntry<String, String>>> REFERENCE_LEVELS = List.of(
        List.of(
            TombstoneEntry.live("AAPL", "listed"),
            TombstoneEntry.live("ENRN", "listed"),
            TombstoneEntry.live("MSFT", "listed")),
        List.of(TombstoneEntry.live("AAPL", "listed-adr")),
        List.of(TombstoneEntry.<String, String>tombstone("ENRN")));

    /**
     * Last-price rows. The flushed level is stale for AAPL; the memtable holds
     * the write that has not reached disk yet.
     */
    private static final List<String> PRICE_SYMBOLS = List.of("AAPL", "MSFT");
    private static final List<Integer> PRICE_FLUSHED = List.of(150, 300);
    private static final int MEMTABLE_AAPL = 152;

    public static void main(String[] args) {
        System.out.println("market-data store: " + VENUE_TAPES.size() + " venue tapes, "
            + BID_LADDERS.size() + " bid ladders, " + REFERENCE_LEVELS.size()
            + " reference levels");
        baseConsolidatedTape();
        sessionWindowScan();
        walkBidLadderDown();
        resolveReferenceRows();
        compactLastPrices();
        memtableWinsTheRead();
    }

    private static List<Iterator<Integer>> tapes() {
        List<Iterator<Integer>> out = new ArrayList<>();
        for (List<Integer> t : VENUE_TAPES) out.add(t.iterator());
        return out;
    }

    /**
     * Base API: each venue publishes trades already sorted by exchange
     * timestamp. Merging their heads on a min-heap gives one chronological
     * consolidated tape without materialising and re-sorting the union.
     */
    static void baseConsolidatedTape() {
        System.out.println("\n== base: consolidated trade tape ==");
        MergeIterator<Integer> merge = new MergeIterator<>(tapes());
        System.out.println("  live venues: " + merge.liveStreams());
        System.out.println("  earliest trade: " + merge.peek());
        List<Integer> tape = new ArrayList<>();
        while (merge.hasNext()) tape.add(merge.next());
        System.out.println("  " + tape.size() + " trades in order: " + tape);
        if (tape.size() != 10) throw new AssertionError("every trade appears once");
        for (int i = 1; i < tape.size(); i++) {
            if (tape.get(i - 1) > tape.get(i)) {
                throw new AssertionError("the tape stays chronological");
            }
        }
    }

    /**
     * seek-to: a regular-session query wants [open, close) and nothing else.
     * seek(open) advances every venue past its pre-market ticks in one bounded
     * reposition; setUpperBound(close) ends the scan, so the caller pulls
     * next() until it stops rather than testing each element itself.
     */
    static void sessionWindowScan() {
        System.out.println("\n== seek-to: one session window out of the tape ==");
        int open = 9_300;
        int close = 9_800;

        SeekableMergeIterator<Integer> scan = new SeekableMergeIterator<>(tapes());
        scan.seek(open);
        scan.setUpperBound(close);

        List<Integer> window = new ArrayList<>();
        while (scan.hasNext()) window.add(scan.next());
        System.out.println("  window [" + open + ", " + close + "): " + window);
        if (!window.equals(List.of(9_300, 9_400, 9_450, 9_600))) {
            throw new AssertionError("half-open: the close tick must be excluded");
        }
    }

    /**
     * reverse: a bid ladder is quoted best-price-first, so it arrives sorted
     * descending already. Merging the ladders descending gives one consolidated
     * book. Pricing a marketable sell only needs the levels between the touch
     * and a limit, so seekForPrev starts the walk and setLowerBound ends it -
     * the rest of the book is never read.
     */
    static void walkBidLadderDown() {
        System.out.println("\n== reverse: walk the consolidated bid ladder down ==");
        List<Iterator<Integer>> ladders = new ArrayList<>();
        for (List<Integer> l : BID_LADDERS) ladders.add(l.iterator());

        ReverseMergeIterator<Integer> book = new ReverseMergeIterator<>(ladders);
        System.out.println("  best bid across venues: " + book.peek());

        int limit = 10_100;
        book.seekForPrev(10_110);
        book.setLowerBound(limit);

        List<Integer> fillable = new ArrayList<>();
        while (book.hasNext()) fillable.add(book.next());
        System.out.println("  levels from 10110 down to the " + limit + " limit: " + fillable);
        if (!fillable.equals(List.of(10_110, 10_105, 10_101))) {
            throw new AssertionError("descending, and the lower bound is inclusive");
        }
    }

    /**
     * tombstones: a reference read across three levels. The newest level's
     * delisting shadows the same symbol everywhere below it, so the key leaves
     * the result entirely; AAPL takes its newer status from the middle level.
     */
    static void resolveReferenceRows() {
        System.out.println("\n== tombstones: resolve the reference rows ==");
        List<Iterator<TombstoneEntry<String, String>>> levels = new ArrayList<>();
        for (List<TombstoneEntry<String, String>> rows : REFERENCE_LEVELS) {
            levels.add(rows.iterator());
        }

        TombstoneMergeIterator<String, String> merge = new TombstoneMergeIterator<>(levels);
        List<String> resolved = new ArrayList<>();
        while (merge.hasNext()) {
            TombstoneEntry<String, String> e = merge.next();
            resolved.add(e.key() + "=" + e.value());
        }
        System.out.println("  live instruments: " + resolved);
        if (!resolved.equals(List.of("AAPL=listed-adr", "MSFT=listed"))) {
            throw new AssertionError("the delisting must shadow, AAPL must take the newer row");
        }
    }

    /**
     * dedup: the same symbol appears in the flushed level and the memtable.
     * Latest-source-wins collapses each symbol to one row, which is the
     * compaction output. Registration order carries the recency here.
     */
    static void compactLastPrices() {
        System.out.println("\n== dedup: compact the last-price rows ==");
        List<DedupEntry<String, Integer>> flushed = new ArrayList<>();
        for (int i = 0; i < PRICE_SYMBOLS.size(); i++) {
            flushed.add(new DedupEntry<>(PRICE_SYMBOLS.get(i), PRICE_FLUSHED.get(i)));
        }
        List<DedupEntry<String, Integer>> memtable =
            List.of(new DedupEntry<>("AAPL", MEMTABLE_AAPL));

        DedupMergeIterator<String, Integer> merge =
            new DedupMergeIterator<>(List.of(flushed.iterator(), memtable.iterator()));
        List<String> compacted = new ArrayList<>();
        while (merge.hasNext()) {
            DedupEntry<String, Integer> e = merge.next();
            compacted.add(e.key() + "=" + e.value());
        }
        System.out.println("  compacted last prices: " + compacted);
        if (!compacted.equals(List.of("AAPL=152", "MSFT=300"))) {
            throw new AssertionError("AAPL must take the memtable price");
        }
    }

    /**
     * priority: the same two sources, registered the other way round - a read
     * path holds the memtable first. Registration order now says the wrong
     * thing, so authority is stated explicitly and the merge still resolves
     * AAPL to the unflushed write.
     */
    static void memtableWinsTheRead() {
        System.out.println("\n== priority: the memtable is authoritative ==");
        PrioritySource<String, Integer> memtable = new PrioritySource<>(
            100, List.of(new PriorityEntry<>("AAPL", MEMTABLE_AAPL)).iterator());
        List<PriorityEntry<String, Integer>> disk = new ArrayList<>();
        for (int i = 0; i < PRICE_SYMBOLS.size(); i++) {
            disk.add(new PriorityEntry<>(PRICE_SYMBOLS.get(i), PRICE_FLUSHED.get(i)));
        }
        PrioritySource<String, Integer> flushed = new PrioritySource<>(10, disk.iterator());

        PriorityMergeIterator<String, Integer> merge =
            new PriorityMergeIterator<>(List.of(memtable, flushed));
        List<String> view = new ArrayList<>();
        while (merge.hasNext()) {
            PriorityEntry<String, Integer> e = merge.next();
            view.add(e.key() + "=" + e.value());
        }
        System.out.println("  resolved read view: " + view);
        if (!view.equals(List.of("AAPL=152", "MSFT=300"))) {
            throw new AssertionError("the memtable must win AAPL despite registering first");
        }
    }
}
