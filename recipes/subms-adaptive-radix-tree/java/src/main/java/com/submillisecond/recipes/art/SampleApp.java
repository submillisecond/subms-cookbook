package com.submillisecond.recipes.art;

import com.submillisecond.recipes.art.features.ArtMetrics;
import com.submillisecond.recipes.art.features.ArtSnapshot;
import com.submillisecond.recipes.art.features.Compaction;
import com.submillisecond.recipes.art.features.MeasuredArt;
import com.submillisecond.recipes.art.features.RangeScan;
import com.submillisecond.recipes.art.features.Serialize;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.List;

/**
 * Sample app: a tour of {@code subms-adaptive-radix-tree}, base API first, then
 * each optional feature. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.art.SampleApp}
 *
 * <p>The running scenario is a venue-qualified instrument dictionary: byte-string
 * symbols ({@code XNAS:AAPL}, {@code XNYS:BRK.A}) map to internal instrument ids.
 * It is the ordered/prefix index a market-data or order-management path keeps in
 * front of the slower reference-data store.
 *
 * <ul>
 *   <li>base             - point lookup of an instrument id by its venue symbol
 *   <li>range-scan       - every instrument listed on one venue, via a prefix range
 *   <li>serialize        - persist the dictionary to bytes and rebuild it (EOD/SOD)
 *   <li>concurrent-reads - a frozen snapshot many pricing readers fan out over
 *   <li>metrics          - per-instance op counters + node-shape census
 *   <li>compaction       - delist instruments, then reclaim the byte paths they held
 * </ul>
 */
public final class SampleApp {

    /** Venue-qualified symbol -> internal instrument id. */
    static final String[] SYMBOLS = {
        "XNAS:AAPL", "XNAS:AMZN", "XNAS:MSFT", "XNAS:NVDA",
        "XNYS:BRK.A", "XNYS:JPM", "XNYS:KO", "XNYS:XOM"
    };
    static final long[] IDS = {6001, 6002, 6003, 6004, 7001, 7002, 7003, 7004};

    static byte[] sym(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }

    static Art<Long> loadDictionary() {
        Art<Long> dict = new Art<>();
        for (int i = 0; i < SYMBOLS.length; i++) {
            dict.insert(sym(SYMBOLS[i]), IDS[i]);
        }
        return dict;
    }

    public static void main(String[] args) throws IOException, InterruptedException {
        baseSymbolLookup();
        rangeScanByVenue();
        serializeRoundTrip();
        concurrentReadsSnapshot();
        metricsCensus();
        compactionAfterDelisting();
    }

    /** Base API: resolve an instrument id from its symbol. A hit returns the id,
     *  an unlisted symbol returns null, and two symbols sharing a venue prefix
     *  both resolve through the one path-compressed node that holds "XNAS:A". */
    static void baseSymbolLookup() {
        System.out.println("== base: symbol -> instrument id ==");
        Art<Long> dict = loadDictionary();

        Long hit = dict.get(sym("XNAS:AAPL"));
        Long miss = dict.get(sym("XNAS:TSLA")); // not listed
        System.out.println("  XNAS:AAPL -> " + hit);
        System.out.println("  XNAS:TSLA -> " + miss);
        if (hit == null || hit != 6001L) throw new AssertionError("listed symbol must resolve");
        if (miss != null) throw new AssertionError("unlisted symbol must miss");

        if (dict.get(sym("XNAS:AMZN")) != 6002L) throw new AssertionError("path-compressed sibling resolves");
        if (dict.size() != SYMBOLS.length) throw new AssertionError("every symbol indexed");
        System.out.println("  " + dict.size() + " symbols indexed");
    }

    /** range-scan: a byte-lex ordered scan between two bounds. The prefix idiom
     *  is [prefix, prefix-with-last-byte-incremented) - here [XNAS:, XNAS;)
     *  captures exactly the venue's listings, in sorted order, pruning the rest. */
    static void rangeScanByVenue() {
        System.out.println("\n== range-scan: all listings on one venue ==");
        Art<Long> dict = loadDictionary();

        List<RangeScan.Entry<Long>> venue =
                RangeScan.range(dict, RangeScan.Bound.included(sym("XNAS:")), RangeScan.Bound.excluded(sym("XNAS;")));
        for (RangeScan.Entry<Long> e : venue) {
            System.out.println("  " + new String(e.key, StandardCharsets.UTF_8) + " -> " + e.value);
        }
        if (venue.size() != 4) throw new AssertionError("exactly the four XNAS listings");
        String first = new String(venue.get(0).key, StandardCharsets.UTF_8);
        if (!first.equals("XNAS:AAPL")) throw new AssertionError("byte-lex sorted, AAPL first");
    }

    /** serialize: dump the whole dictionary to bytes at end of day and rebuild it
     *  at start of day. The INT64 value codec ships in the box; the round-trip
     *  preserves every listing. */
    static void serializeRoundTrip() throws IOException {
        System.out.println("\n== serialize: persist and reload the dictionary ==");
        Art<Long> dict = loadDictionary();

        byte[] bytes = Serialize.writeToBytes(dict, Serialize.INT64);
        System.out.println("  " + dict.size() + " symbols -> " + bytes.length + " bytes");

        Art<Long> restored = Serialize.parseBytes(bytes, Serialize.INT64);
        if (restored.size() != dict.size()) throw new AssertionError("size survives the round trip");
        for (int i = 0; i < SYMBOLS.length; i++) {
            if (restored.get(sym(SYMBOLS[i])) != IDS[i]) throw new AssertionError("listing survives: " + SYMBOLS[i]);
        }
        System.out.println("  reloaded and verified " + restored.size() + " symbols");
    }

    /** concurrent-reads: freeze the dictionary into an ArtSnapshot that pricing /
     *  risk reader threads share lock-free while the loader keeps ingesting new
     *  listings. The snapshot is a point-in-time view, unaffected by later writes. */
    static void concurrentReadsSnapshot() throws InterruptedException {
        System.out.println("\n== concurrent-reads: lock-free reader fan-out ==");
        Art<Long> dict = loadDictionary();
        ArtSnapshot<Long> snap = ArtSnapshot.fromTree(dict);

        // Loader lists a new instrument after the snapshot was taken.
        dict.insert(sym("XNAS:TSLA"), 6005L);

        int[] hits = new int[2];
        Thread[] readers = new Thread[2];
        for (int t = 0; t < readers.length; t++) {
            final int slot = t;
            readers[t] = new Thread(() -> {
                int count = 0;
                for (String s : SYMBOLS) {
                    if (snap.get(sym(s)) != null) count++;
                }
                hits[slot] = count;
            });
            readers[t].start();
        }
        for (Thread r : readers) r.join();

        System.out.println("  each reader resolved " + hits[0] + " of " + SYMBOLS.length + " symbols");
        for (int h : hits) {
            if (h != SYMBOLS.length) throw new AssertionError("reader saw every pre-snapshot symbol");
        }
        if (snap.get(sym("XNAS:TSLA")) != null) throw new AssertionError("post-snapshot listing invisible");
        System.out.println("  post-snapshot listing invisible to readers, as intended");
    }

    /** metrics: MeasuredArt bumps per-op counters and, on demand, walks the tree
     *  for its Node4/16/48/256 census - the shape a live index takes. */
    static void metricsCensus() {
        System.out.println("\n== metrics: op counters + node-shape census ==");
        MeasuredArt<Long> dict = new MeasuredArt<>();
        for (int i = 0; i < SYMBOLS.length; i++) {
            dict.insert(sym(SYMBOLS[i]), IDS[i]);
        }
        dict.get(sym("XNYS:JPM")); // hit
        dict.get(sym("XNYS:GS"));  // miss

        ArtMetrics m = dict.metrics();
        System.out.println("  inserts=" + m.insertions + " lookups=" + m.lookups + " entries=" + m.entries);
        System.out.println("  nodes: n4=" + m.node4 + " n16=" + m.node16 + " n48=" + m.node48 + " n256=" + m.node256);
        if (m.insertions != SYMBOLS.length) throw new AssertionError("one insert per symbol");
        if (m.lookups != 2) throw new AssertionError("two lookups counted");
        if (m.entries != SYMBOLS.length) throw new AssertionError("every symbol still keyed");
    }

    /** compaction: delete clears a value but leaves its byte path in place;
     *  compact is the periodic sweep that prunes those now-empty paths and demotes
     *  over-sized nodes. Run it after a bulk delisting, not per delete. */
    static void compactionAfterDelisting() {
        System.out.println("\n== compaction: reclaim delisted instrument paths ==");
        Art<Long> dict = loadDictionary();

        String[] delisted = {"XNYS:KO", "XNYS:XOM"};
        if (Compaction.delete(dict, sym("XNYS:KO")) != 7003L) throw new AssertionError("delete returns the prior id");
        if (Compaction.delete(dict, sym("XNYS:XOM")) != 7004L) throw new AssertionError("delete returns the prior id");
        int changes = Compaction.compact(dict);
        System.out.println("  delisted " + delisted.length + " symbols, compaction made " + changes + " structural changes");
        if (changes <= 0) throw new AssertionError("compaction reports structural changes");

        for (String s : delisted) {
            if (dict.get(sym(s)) != null) throw new AssertionError("delisted symbol no longer resolves: " + s);
        }
        if (dict.get(sym("XNAS:AAPL")) != 6001L) throw new AssertionError("surviving listing still resolves");
        if (dict.size() != SYMBOLS.length - delisted.length) throw new AssertionError("size reflects the delisting");
        System.out.println("  " + dict.size() + " symbols remain");
    }
}
