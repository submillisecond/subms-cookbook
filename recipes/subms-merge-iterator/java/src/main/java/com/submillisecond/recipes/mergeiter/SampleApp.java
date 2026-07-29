package com.submillisecond.recipes.mergeiter;

import com.submillisecond.recipes.mergeiter.features.DedupEntry;
import com.submillisecond.recipes.mergeiter.features.DedupMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PriorityEntry;
import com.submillisecond.recipes.mergeiter.features.PriorityMergeIterator;
import com.submillisecond.recipes.mergeiter.features.PrioritySource;
import com.submillisecond.recipes.mergeiter.features.SeekableMergeIterator;
import com.submillisecond.recipes.mergeiter.features.TombstoneEntry;
import com.submillisecond.recipes.mergeiter.features.TombstoneMergeIterator;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

/**
 * Sample app: a tour of {@code subms-merge-iterator}, base API first, then each
 * feature variant. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.mergeiter.SampleApp}
 *
 * <p>The framing is an LSM-backed market-data store. The base merge
 * consolidates per-venue sorted trade streams into one time-ordered tape; each
 * feature is a piece of the read path over that store.
 *
 * <ul>
 *   <li>base       - consolidate per-venue sorted trade timestamps into one tape
 *   <li>seek-to    - skip the pre-market ticks, start the scan at the session open
 *   <li>tombstones - an instrument-reference read where a delisting shadows the row
 *   <li>dedup      - compact append-only price shards to the freshest price per symbol
 *   <li>priority   - a live memtable outranks stale on-disk SSTable levels on a key tie
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) {
        baseConsolidatedTape();
        seekToSessionOpen();
        tombstoneReferenceRead();
        dedupPriceShards();
        priorityMemtableWins();
    }

    /** Base API: merge three venues' sorted trade timestamps into one tape. */
    static void baseConsolidatedTape() {
        System.out.println("== base: consolidated trade tape ==");
        List<Iterator<Integer>> venues = List.of(
            List.of(100, 450, 780).iterator(),
            List.of(120, 460, 810).iterator(),
            List.of(90, 300, 900).iterator());
        MergeIterator<Integer> tape = new MergeIterator<>(venues);
        List<Integer> out = new ArrayList<>();
        while (tape.hasNext()) out.add(tape.next());
        System.out.println("  merged " + out.size() + " trades: " + out);
        if (out.size() != 9) throw new AssertionError("every trade appears once");
        for (int i = 1; i < out.size(); i++) {
            if (out.get(i - 1) > out.get(i)) throw new AssertionError("tape is chronological");
        }
        if (out.get(0) != 90) throw new AssertionError("earliest trade leads the tape");
    }

    /** seek-to: skip past the pre-market ticks to the first trade at the open. */
    static void seekToSessionOpen() {
        System.out.println("\n== seek-to: skip to the session open ==");
        List<Iterator<Integer>> venues = List.of(
            List.of(8_000, 9_100, 9_400, 9_800).iterator(),
            List.of(8_500, 9_300, 9_600).iterator());
        SeekableMergeIterator<Integer> scan = new SeekableMergeIterator<>(venues);
        int open = 9_300;
        scan.seek(open);
        List<Integer> session = new ArrayList<>();
        while (scan.hasNext()) session.add(scan.next());
        System.out.println("  first regular-session trades: " + session);
        if (session.get(0) != 9_300) throw new AssertionError("scan starts at the open");
        for (int t : session) {
            if (t < open) throw new AssertionError("no pre-market ticks leak in");
        }
    }

    /** tombstones: a newer-level delisting shadows the same symbol in older levels. */
    static void tombstoneReferenceRead() {
        System.out.println("\n== tombstones: a delisting shadows older rows ==");
        List<TombstoneEntry<String, String>> older = List.of(
            TombstoneEntry.live("AAPL", "active"),
            TombstoneEntry.live("ENRN", "active"));
        List<TombstoneEntry<String, String>> newer = List.of(TombstoneEntry.tombstone("ENRN"));
        TombstoneMergeIterator<String, String> merge =
            new TombstoneMergeIterator<>(List.of(older.iterator(), newer.iterator()));
        List<String> live = new ArrayList<>();
        while (merge.hasNext()) live.add(merge.next().key());
        System.out.println("  symbols in the read result: " + live);
        if (!live.equals(List.of("AAPL"))) {
            throw new AssertionError("the delisted symbol must be shadowed out");
        }
    }

    /** dedup: collapse append-only price shards to the freshest price per symbol. */
    static void dedupPriceShards() {
        System.out.println("\n== dedup: freshest price per symbol ==");
        List<DedupEntry<String, Integer>> olderShard = List.of(
            new DedupEntry<>("AAPL", 150),
            new DedupEntry<>("MSFT", 300));
        List<DedupEntry<String, Integer>> newerShard = List.of(new DedupEntry<>("AAPL", 152));
        DedupMergeIterator<String, Integer> merge =
            new DedupMergeIterator<>(List.of(olderShard.iterator(), newerShard.iterator()));
        List<String> compacted = new ArrayList<>();
        while (merge.hasNext()) {
            DedupEntry<String, Integer> e = merge.next();
            compacted.add(e.key() + "=" + e.value());
        }
        System.out.println("  compacted book: " + compacted);
        if (!compacted.equals(List.of("AAPL=152", "MSFT=300"))) {
            throw new AssertionError("AAPL must take the newer price");
        }
    }

    /** priority: a high-priority memtable beats lower-priority on-disk levels. */
    static void priorityMemtableWins() {
        System.out.println("\n== priority: the memtable beats the SSTables ==");
        PrioritySource<String, String> memtable = new PrioritySource<>(
            100, List.of(new PriorityEntry<>("AAPL", "live-153")).iterator());
        PrioritySource<String, String> sstable = new PrioritySource<>(
            10, List.of(
                new PriorityEntry<>("AAPL", "disk-150"),
                new PriorityEntry<>("MSFT", "disk-300")).iterator());
        PriorityMergeIterator<String, String> merge =
            new PriorityMergeIterator<>(List.of(memtable, sstable));
        List<String> view = new ArrayList<>();
        while (merge.hasNext()) {
            PriorityEntry<String, String> e = merge.next();
            view.add(e.key() + "=" + e.value());
        }
        System.out.println("  resolved read view: " + view);
        if (!view.equals(List.of("AAPL=live-153", "MSFT=disk-300"))) {
            throw new AssertionError("the live memtable must win AAPL");
        }
    }
}
