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
import org.junit.jupiter.api.Test;

import java.util.ArrayList;
import java.util.Iterator;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        List<Iterator<Integer>> streams = List.of(
            List.of(1, 4, 7).iterator(),
            List.of(2, 5, 8).iterator(),
            List.of(3, 6, 9).iterator());
        MergeIterator<Integer> merged = new MergeIterator<>(streams);
        assertEquals(1, merged.peek());                          // head, not consumed
        assertEquals(3, merged.liveStreams());
        List<Integer> out = new ArrayList<>();
        while (merged.hasNext()) out.add(merged.next());
        assertEquals(List.of(1, 2, 3, 4, 5, 6, 7, 8, 9), out);   // one sorted union out
        // quickstart:end
    }

    @Test
    void consolidatedTapeScenario() {
        List<Iterator<Integer>> venues = List.of(
            List.of(100, 450, 780).iterator(),
            List.of(120, 460, 810).iterator(),
            List.of(90, 300, 900).iterator());
        MergeIterator<Integer> tape = new MergeIterator<>(venues);
        List<Integer> out = new ArrayList<>();
        while (tape.hasNext()) out.add(tape.next());
        assertEquals(9, out.size(), "every trade appears once");
        for (int i = 1; i < out.size(); i++) {
            assertTrue(out.get(i - 1) <= out.get(i), "tape stays chronological");
        }
        assertEquals(90, out.get(0), "earliest trade leads");
        assertEquals(900, out.get(out.size() - 1), "latest trade trails");
    }

    @Test
    void seekAndUpperBoundReadOneSessionWindow() {
        List<Iterator<Integer>> venues = List.of(
            List.of(8_000, 9_100, 9_400, 9_800).iterator(),
            List.of(8_500, 9_300, 9_600).iterator(),
            List.of(9_050, 9_450, 9_900).iterator());
        SeekableMergeIterator<Integer> scan = new SeekableMergeIterator<>(venues);
        scan.seek(9_300);
        scan.setUpperBound(9_800);
        List<Integer> window = new ArrayList<>();
        while (scan.hasNext()) window.add(scan.next());
        assertEquals(List.of(9_300, 9_400, 9_450, 9_600), window,
            "starts at the open and stops before the close");
    }

    @Test
    void reverseWalksTheBidLadderDown() {
        List<Iterator<Integer>> ladders = List.of(
            List.of(10_120, 10_105, 10_101, 10_095).iterator(),
            List.of(10_118, 10_110, 10_099).iterator());
        ReverseMergeIterator<Integer> book = new ReverseMergeIterator<>(ladders);
        assertEquals(10_120, book.peek(), "best bid leads the walk");
        book.seekForPrev(10_110);
        book.setLowerBound(10_100);
        List<Integer> fillable = new ArrayList<>();
        while (book.hasNext()) fillable.add(book.next());
        assertEquals(List.of(10_110, 10_105, 10_101), fillable);
    }

    @Test
    void tombstoneShadowsDelistedSymbol() {
        List<TombstoneEntry<String, String>> older = List.of(
            TombstoneEntry.live("AAPL", "active"),
            TombstoneEntry.live("ENRN", "active"));
        List<TombstoneEntry<String, String>> newer = List.of(TombstoneEntry.tombstone("ENRN"));
        TombstoneMergeIterator<String, String> merge =
            new TombstoneMergeIterator<>(List.of(older.iterator(), newer.iterator()));
        List<String> live = new ArrayList<>();
        while (merge.hasNext()) live.add(merge.next().key());
        assertEquals(List.of("AAPL"), live, "the delisted symbol is dropped");
    }

    @Test
    void dedupKeepsFreshestPrice() {
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
        assertEquals(List.of("AAPL=152", "MSFT=300"), compacted);
    }

    @Test
    void priorityMemtableOutranksDisk() {
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
        assertEquals(List.of("AAPL=live-153", "MSFT=disk-300"), view);
    }
}
