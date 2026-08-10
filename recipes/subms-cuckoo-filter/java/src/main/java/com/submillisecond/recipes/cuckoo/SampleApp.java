package com.submillisecond.recipes.cuckoo;

import com.submillisecond.recipes.cuckoo.features.CompressedCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.CuckooSnapshot;
import com.submillisecond.recipes.cuckoo.features.DynamicCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;

/**
 * Sample app: a miniature order-management gateway built on
 * {@code subms-cuckoo-filter}, then a tour of each optional feature. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.cuckoo.SampleApp}
 *
 * <ul>
 *   <li>base                 - an OMS live-order set driven by a drop-copy event stream
 *   <li>base                 - checkpoint, shard fan-in and session roll on the same set
 *   <li>variable-fingerprint - widen the fingerprint to cut false positives on a risk pre-check
 *   <li>dynamic              - an intraday dedup window that grows past its initial sizing
 *   <li>concurrent-reads     - a reader fleet fanning out over a frozen snapshot
 *   <li>compressed-buckets   - a smaller serialized footprint at moderate load
 * </ul>
 */
public final class SampleApp {

    /** One line off a drop-copy stream. */
    private enum Kind { NEW, FILL, CANCEL }

    private record Event(Kind kind, String orderId) { }

    public static void main(String[] args) throws InterruptedException, IOException {
        CuckooFilter open = omsGateway();
        checkpointAndRestore(open);
        shardFanIn();
        sessionRoll(open);
        variableFingerprintRiskPrecheck();
        dynamicDedupWindow();
        concurrentReadsMarketFanout();
        compressedPersistence();
    }

    /**
     * The system: an order gateway replays a drop-copy stream into a live-order
     * set. Every inbound amend is gated on {@code contains} before it costs an
     * authoritative book lookup; a fill or cancel deletes the id. The delete is
     * the move a bloom filter cannot make, and without it the set would grow
     * all session and every closed order would keep answering yes.
     */
    static CuckooFilter omsGateway() {
        System.out.println("== OMS gateway: live-order set from a drop-copy stream ==");
        Event[] stream = {
            new Event(Kind.NEW, "ORD-1001"),
            new Event(Kind.NEW, "ORD-1002"),
            new Event(Kind.NEW, "ORD-1003"),
            new Event(Kind.NEW, "ORD-1004"),
            new Event(Kind.FILL, "ORD-1002"),
            new Event(Kind.NEW, "ORD-1005"),
            new Event(Kind.CANCEL, "ORD-1003"),
            new Event(Kind.FILL, "ORD-1005"),
            new Event(Kind.NEW, "ORD-1004"), // the session resend replays one we already hold
        };

        CuckooFilter open = new CuckooFilter(10_000);
        int opened = 0, replayed = 0, closed = 0;
        for (Event e : stream) {
            if (e.kind() == Kind.NEW) {
                // insertIfAbsent makes the replay idempotent: a resent NEW for
                // a live order must not add a second fingerprint.
                if (open.insertIfAbsent(e.orderId())) {
                    opened++;
                } else {
                    replayed++;
                }
            } else if (open.delete(e.orderId())) {
                closed++;
            }
        }

        System.out.printf("  %d new, %d replayed, %d closed -> %d live%n",
            opened, replayed, closed, open.size());
        System.out.printf("  load %.4f, false-positive rate %.6f%n",
            open.loadFactor(), open.estimatedFpp());

        String amend = "ORD-1002";
        System.out.println("  amend for " + amend + " -> "
            + (open.contains(amend) ? "book lookup" : "reject, already closed"));

        if (open.size() != 2) throw new AssertionError("two orders should remain live");
        if (open.contains("ORD-1002")) throw new AssertionError("a filled order leaves the set");
        if (!open.contains("ORD-1001")) throw new AssertionError("no false negative on a live order");
        return open;
    }

    /**
     * Checkpoint the live set to bytes and reload it. A gateway restarting
     * mid-session rebuilds membership from the last checkpoint instead of
     * replaying the whole day's drop copy.
     */
    static void checkpointAndRestore(CuckooFilter open) throws IOException {
        System.out.println("\n== checkpoint: serialise the live set and reload it ==");
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (DataOutputStream out = new DataOutputStream(bytes)) {
            open.writeTo(out);
        }
        byte[] buf = bytes.toByteArray();
        CuckooFilter restored = CuckooFilter.parse(buf, 0, buf.length);
        System.out.printf("  %d bytes on the wire, %d live orders restored%n",
            buf.length, restored.size());
        if (!restored.contains("ORD-1001")) throw new AssertionError("lost ORD-1001 across the wire");
        if (restored.size() != open.size()) throw new AssertionError("count did not survive");
    }

    /**
     * Fan-in: two gateway shards each hold their own live-order set, and the
     * surveillance process merges them into one. {@code union} re-places every
     * fingerprint rather than OR-ing bit arrays, so both filters must share a
     * geometry - which is why both are built with the same capacity.
     */
    static void shardFanIn() {
        System.out.println("\n== fan-in: merge two shards' live-order sets ==");
        CuckooFilter shardA = new CuckooFilter(10_000);
        CuckooFilter shardB = new CuckooFilter(10_000);
        for (int i = 0; i < 500; i++) {
            shardA.insert("A-ORD-" + i);
            shardB.insert("B-ORD-" + i);
        }
        shardA.union(shardB);
        System.out.println("  merged set holds " + shardA.size() + " orders");
        if (!shardA.contains("A-ORD-7") || !shardA.contains("B-ORD-7")) {
            throw new AssertionError("merge dropped an order");
        }

        CuckooFilter mismatched = new CuckooFilter(1_000_000);
        try {
            shardA.union(mismatched);
            throw new AssertionError("a differently-sized shard must be refused");
        } catch (CuckooException e) {
            System.out.println("  merging a differently-sized shard -> " + e.reason());
        }
    }

    /**
     * Session roll: {@code clear} zeroes the set at the close and keeps the
     * allocation, so tomorrow's first order does not pay for a fresh 16 KB
     * array.
     */
    static void sessionRoll(CuckooFilter open) {
        System.out.println("\n== session roll: clear and reuse the allocation ==");
        long bytes = open.sizeInBytes();
        open.clear();
        System.out.printf("  after close: %d live, %d bytes still held%n", open.size(), bytes);
        if (!open.isEmpty()) throw new AssertionError("clear should empty the set");
        if (open.contains("ORD-1001")) throw new AssertionError("clear should drop every key");
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
    static void compressedPersistence() throws IOException {
        System.out.println("\n== compressed-buckets: smaller serialized footprint at moderate load ==");
        CompressedCuckooFilter cf = new CompressedCuckooFilter(10_000);
        for (int i = 0; i < 3_000; i++) cf.insert("ORD-" + i);
        int baseFixedBytes = cf.bucketCount() * 4; // base layout: 4 slot bytes per bucket
        ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        try (DataOutputStream out = new DataOutputStream(bytes)) {
            cf.writeTo(out);
        }
        byte[] buf = bytes.toByteArray();
        System.out.printf("  serialised %d bytes, base fixed-array layout would be %d%n",
            buf.length, baseFixedBytes + 17);
        CompressedCuckooFilter reloaded = CompressedCuckooFilter.parse(buf, 0, buf.length);
        if (reloaded.size() != cf.size()) throw new AssertionError("count did not survive");
        if (cf.occupiedBytes() >= baseFixedBytes) throw new AssertionError("sorted-run encoding wins at moderate load");
        for (int i = 0; i < 3_000; i++) {
            if (!cf.contains("ORD-" + i)) throw new AssertionError("lost ORD-" + i);
        }
        if (!cf.delete("ORD-0")) throw new AssertionError("delete still works on the compressed layout");
    }
}
