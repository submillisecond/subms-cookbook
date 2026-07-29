package com.submillisecond.recipes.cuckoo;

import com.submillisecond.recipes.cuckoo.features.CompressedCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.CuckooSnapshot;
import com.submillisecond.recipes.cuckoo.features.DynamicCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter;
import com.submillisecond.recipes.cuckoo.features.VariableFpCuckooFilter.FingerprintWidth;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        CuckooFilter cf = new CuckooFilter(10_000);
        cf.insert("alice");
        assertTrue(cf.contains("alice"));
        assertTrue(cf.delete("alice"));   // the move a bloom filter cannot make
        assertFalse(cf.contains("alice"));
        // quickstart:end
    }

    @Test
    void openOrderSetScenario() {
        CuckooFilter open = new CuckooFilter(10_000);
        for (String oid : new String[] {"ORD-1001", "ORD-1002", "ORD-1003", "ORD-1004"}) {
            open.insert(oid);
        }
        assertEquals(4, open.size());

        assertTrue(open.contains("ORD-1002"));
        assertTrue(open.delete("ORD-1002"), "a live order can be deleted");
        assertFalse(open.contains("ORD-1002"), "a filled order leaves the live set");

        for (String oid : new String[] {"ORD-1001", "ORD-1003", "ORD-1004"}) {
            assertTrue(open.contains(oid), "a stored order must always report present");
        }
        assertEquals(3, open.size());
    }

    @Test
    void variableFingerprintLowersFalsePositives() {
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
        assertTrue(wideFp < narrowFp, "wideFp=" + wideFp + " should be < narrowFp=" + narrowFp);
    }

    @Test
    void dynamicGrowsAndKeepsEveryId() {
        DynamicCuckooFilter seen = new DynamicCuckooFilter(1_000, 0.5);
        for (int i = 0; i < 20_000; i++) seen.insert("MSG-" + i);
        assertTrue(seen.layerCount() > 1, "grew past the initial layer");
        for (int i = 0; i < 20_000; i++) {
            assertTrue(seen.contains("MSG-" + i), "no id dropped as the window grew");
        }
    }

    @Test
    void snapshotIsFrozenAgainstLaterWrites() throws InterruptedException {
        CuckooFilter open = new CuckooFilter(10_000);
        for (int i = 0; i < 1_000; i++) open.insert("ORD-" + i);
        CuckooSnapshot snap = CuckooSnapshot.capture(open);

        open.insert("ORD-LATE");
        open.delete("ORD-0");

        int[] matched = new int[4];
        Thread[] readers = new Thread[matched.length];
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
        for (int m : matched) assertEquals(1_000, m, "the snapshot keeps the whole captured set");
        assertTrue(snap.contains("ORD-0"), "snapshot retains a key the writer later deleted");
        assertFalse(snap.contains("ORD-LATE"), "snapshot never sees the writer's later insert");
    }

    @Test
    void compressedFootprintBeatsBaseAtModerateLoad() {
        CompressedCuckooFilter cf = new CompressedCuckooFilter(10_000);
        for (int i = 0; i < 3_000; i++) cf.insert("ORD-" + i);
        int baseFixedBytes = cf.bucketCount() * 4;
        assertTrue(cf.occupiedBytes() < baseFixedBytes, "sorted-run encoding is smaller here");
        for (int i = 0; i < 3_000; i++) {
            assertTrue(cf.contains("ORD-" + i));
        }
        assertTrue(cf.delete("ORD-0"));
    }
}
