package com.submillisecond.recipes.art;

import com.submillisecond.recipes.art.features.ArtMetrics;
import com.submillisecond.recipes.art.features.ArtSnapshot;
import com.submillisecond.recipes.art.features.Compaction;
import com.submillisecond.recipes.art.features.MeasuredArt;
import com.submillisecond.recipes.art.features.RangeScan;
import com.submillisecond.recipes.art.features.Serialize;
import org.junit.jupiter.api.Test;

import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.List;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotNull;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    private static byte[] b(String s) {
        return s.getBytes(StandardCharsets.UTF_8);
    }

    @Test
    void quickstart() {
        // quickstart:begin
        Art<Integer> t = new Art<>();
        t.insert("alice".getBytes(), 1);
        t.insert("alicia".getBytes(), 2); // shares "ali", splits at the 4th byte
        assertEquals(1, t.get("alice".getBytes()));
        assertNull(t.get("missing".getBytes()));
        // quickstart:end
    }

    @Test
    void baseSymbolLookupScenario() {
        Art<Long> dict = SampleApp.loadDictionary();
        assertEquals(6001L, dict.get(b("XNAS:AAPL")), "listed symbol resolves");
        assertNull(dict.get(b("XNAS:TSLA")), "unlisted symbol misses");
        assertEquals(6002L, dict.get(b("XNAS:AMZN")), "path-compressed sibling resolves");
        assertEquals(SampleApp.SYMBOLS.length, dict.size());
    }

    @Test
    void rangeScanPrefixSelectsOneVenue() {
        Art<Long> dict = SampleApp.loadDictionary();
        List<RangeScan.Entry<Long>> venue =
                RangeScan.range(dict, RangeScan.Bound.included(b("XNAS:")), RangeScan.Bound.excluded(b("XNAS;")));
        assertEquals(4, venue.size(), "exactly the four XNAS listings");
        assertEquals("XNAS:AAPL", new String(venue.get(0).key, StandardCharsets.UTF_8), "byte-lex sorted");
        assertEquals("XNAS:NVDA", new String(venue.get(3).key, StandardCharsets.UTF_8));
    }

    @Test
    void serializeRoundTripsEveryListing() throws IOException {
        Art<Long> dict = SampleApp.loadDictionary();
        byte[] bytes = Serialize.writeToBytes(dict, Serialize.INT64);
        Art<Long> restored = Serialize.parseBytes(bytes, Serialize.INT64);
        assertEquals(dict.size(), restored.size());
        for (int i = 0; i < SampleApp.SYMBOLS.length; i++) {
            assertEquals(SampleApp.IDS[i], restored.get(b(SampleApp.SYMBOLS[i])), SampleApp.SYMBOLS[i]);
        }
    }

    @Test
    void snapshotIsFrozenAgainstLaterWrites() {
        Art<Long> dict = SampleApp.loadDictionary();
        ArtSnapshot<Long> snap = ArtSnapshot.fromTree(dict);
        dict.insert(b("XNAS:TSLA"), 6005L);
        assertEquals(6001L, snap.get(b("XNAS:AAPL")), "pre-snapshot listing visible");
        assertNull(snap.get(b("XNAS:TSLA")), "post-snapshot listing invisible");
        assertEquals(SampleApp.SYMBOLS.length, snap.size());
    }

    @Test
    void metricsTrackTheOpMix() {
        MeasuredArt<Long> dict = new MeasuredArt<>();
        for (int i = 0; i < SampleApp.SYMBOLS.length; i++) {
            dict.insert(b(SampleApp.SYMBOLS[i]), SampleApp.IDS[i]);
        }
        assertNotNull(dict.get(b("XNYS:JPM")));
        assertNull(dict.get(b("XNYS:GS")));
        ArtMetrics m = dict.metrics();
        assertEquals(SampleApp.SYMBOLS.length, m.insertions);
        assertEquals(2, m.lookups);
        assertEquals(SampleApp.SYMBOLS.length, m.entries);
    }

    @Test
    void compactionReclaimsDelistedPaths() {
        Art<Long> dict = SampleApp.loadDictionary();
        assertEquals(7003L, Compaction.delete(dict, b("XNYS:KO")));
        assertEquals(7004L, Compaction.delete(dict, b("XNYS:XOM")));
        int changes = Compaction.compact(dict);
        assertTrue(changes > 0, "compaction reports structural changes after a delisting");
        assertNull(dict.get(b("XNYS:KO")));
        assertNull(dict.get(b("XNYS:XOM")));
        assertEquals(6001L, dict.get(b("XNAS:AAPL")), "survivors still resolve");
        assertEquals(SampleApp.SYMBOLS.length - 2, dict.size());
    }
}
