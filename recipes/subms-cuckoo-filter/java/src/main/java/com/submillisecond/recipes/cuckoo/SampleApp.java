package com.submillisecond.recipes.cuckoo;

import com.submillisecond.recipes.cuckoo.features.CompressedCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.CuckooSnapshot;
import com.submillisecond.recipes.cuckoo.features.DynamicCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;

/**
 * Sample app: a tour of {@code subms-cuckoo-filter}, base API first, then each
 * optional feature. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.cuckoo.SampleApp}
 *
 * <ul>
 *   <li>base                 - a live-order membership set where fills delete entries
 *   <li>variable-fingerprint - widen the fingerprint to cut false positives on a risk pre-check
 *   <li>dynamic              - an intraday dedup window that grows past its initial sizing
 *   <li>concurrent-reads     - a reader fleet fanning out over a frozen snapshot
 *   <li>compressed-buckets   - a smaller serialized footprint at moderate load
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) throws InterruptedException {
        baseOpenOrderSet();
        variableFingerprintRiskPrecheck();
        dynamicDedupWindow();
        concurrentReadsMarketFanout();
        compressedPersistence();
    }

    /** Base API: an OMS keeps the set of open order IDs; a fill or cancel deletes one. */
    static void baseOpenOrderSet() {
        System.out.println("== base: open-order membership set ==");
        CuckooFilter open = new CuckooFilter(10_000);
        for (String oid : new String[] {"ORD-1001", "ORD-1002", "ORD-1003", "ORD-1004"}) {
            open.insert(oid);
        }
        System.out.println("  live orders: " + open.size());

        if (!open.contains("ORD-1002")) throw new AssertionError("ORD-1002 should be live");
        open.delete("ORD-1002"); // a fill closes it out
        System.out.println("  after fill on ORD-1002 -> still live? " + open.contains("ORD-1002"));

        if (open.contains("ORD-1002")) throw new AssertionError("a filled order leaves the live set");
        if (!open.contains("ORD-1001")) throw new AssertionError("no false negatives for open orders");
        if (open.size() != 3) throw new AssertionError("count should drop to 3");
    }

    /** variable-fingerprint: widen 8 -> 16 bits to shrink the false-positive rate. */
    static void variableFingerprintRiskPrecheck() {
        System.out.println("\n== variable-fingerprint: cut false positives on a risk pre-check ==");
        int n = 5_000;
        VariableFpCuckooFilter narrow = new VariableFpCuckooFilter(n, FingerprintWidth.EIGHT);
        VariableFpCuckooFilter wide = new VariableFpCuckooFilter(n, FingerprintWidth.SIXTEEN);
        for (int i = 0; i < n; i++) {
            narrow.insert("RESTRICTED-" + i);
            wide.insert("RESTRICTED-" + i);
        }
        int narrowFp = 0, wideFp = 0;
        for (int i = 0; i < 10_000; i++) {
            String sym = "TRADABLE-" + i;
            if (narrow.contains(sym)) narrowFp++;
            if (wide.contains(sym)) wideFp++;
        }
        System.out.println("  8-bit false positives:  " + narrowFp);
        System.out.println("  16-bit false positives: " + wideFp);
        if (wideFp >= narrowFp) throw new AssertionError("wider fingerprint lowers the false-positive rate");
    }

    /** dynamic: chain a fresh layer as load climbs so a late-session id is never dropped. */
    static void dynamicDedupWindow() {
        System.out.println("\n== dynamic: an intraday dedup window that grows itself ==");
        DynamicCuckooFilter seen = new DynamicCuckooFilter(1_000, 0.5);
        for (int i = 0; i < 20_000; i++) seen.insert("MSG-" + i);
        System.out.printf("  20k ids -> %d layers, active load %.2f%n", seen.layerCount(), seen.loadFactor());
        if (seen.layerCount() <= 1) throw new AssertionError("the window grew past its initial sizing");
        for (int i = 0; i < 20_000; i++) {
            if (!seen.contains("MSG-" + i)) throw new AssertionError("no id dropped as the window grew");
        }
    }

    /** concurrent-reads: readers fan out over a frozen snapshot while the writer keeps mutating. */
    static void concurrentReadsMarketFanout() throws InterruptedException {
        System.out.println("\n== concurrent-reads: a reader fleet over a frozen open-order set ==");
        CuckooFilter open = new CuckooFilter(10_000);
        for (int i = 0; i < 1_000; i++) open.insert("ORD-" + i);
        CuckooSnapshot snap = CuckooSnapshot.capture(open);

        // The writer keeps mutating after the snapshot is frozen.
        open.insert("ORD-LATE");
        open.delete("ORD-0");

        Thread[] readers = new Thread[4];
        int[] matched = new int[readers.length];
        for (int t = 0; t < readers.length; t++) {
            final int idx = t;
            readers[t] = new Thread(() -> {
                int found = 0;
                for (int i = 0; i < 1_000; i++) {
                    if (snap.contains("ORD-" + i)) found++;
                }
                matched[idx] = found;
            });
            readers[t].start();
        }
        for (Thread r : readers) r.join();
        for (int m : matched) {
            if (m != 1_000) throw new AssertionError("every reader sees the whole frozen set");
        }
        System.out.println("  4 readers each matched all 1000 orders in the snapshot");
        if (!snap.contains("ORD-0")) throw new AssertionError("snapshot keeps its pre-freeze state");
        if (snap.contains("ORD-LATE")) throw new AssertionError("snapshot does not see the later insert");
    }

    /** compressed-buckets: the sorted-run encoding shrinks the serialized footprint at moderate load. */
    static void compressedPersistence() {
        System.out.println("\n== compressed-buckets: smaller serialized footprint at moderate load ==");
        CompressedCuckooFilter cf = new CompressedCuckooFilter(10_000);
        for (int i = 0; i < 3_000; i++) cf.insert("ORD-" + i);
        int baseFixedBytes = cf.bucketCount() * 4; // base layout: 4 slot bytes per bucket
        System.out.println("  live bytes " + cf.occupiedBytes() + ", base fixed-array bytes " + baseFixedBytes);
        if (cf.occupiedBytes() >= baseFixedBytes) throw new AssertionError("sorted-run encoding wins at moderate load");
        for (int i = 0; i < 3_000; i++) {
            if (!cf.contains("ORD-" + i)) throw new AssertionError("lost ORD-" + i);
        }
        if (!cf.delete("ORD-0")) throw new AssertionError("delete still works on the compressed layout");
    }
}
