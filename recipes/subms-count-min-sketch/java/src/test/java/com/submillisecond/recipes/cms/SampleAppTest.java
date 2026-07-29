package com.submillisecond.recipes.cms;

import com.submillisecond.recipes.cms.features.HeavyHitters;
import com.submillisecond.recipes.cms.features.Merge;
import com.submillisecond.recipes.cms.features.WindowedCountMinSketch;
import org.junit.jupiter.api.Test;

import java.util.HashMap;
import java.util.List;
import java.util.Map;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        CountMinSketch cms = new CountMinSketch(5, 16384);
        for (int i = 0; i < 1000; i++) {
            cms.add("hello");
        }
        assertTrue(cms.estimate("hello") >= 1000);   // estimate never below true
        assertEquals(0, cms.estimate("never-seen")); // absent key reads 0 barring a collision
        // quickstart:end
    }

    @Test
    void baseNeverUnderestimates() {
        var stream = SampleApp.marketStream();
        Map<String, Integer> exact = new HashMap<>();
        CountMinSketch cms = new CountMinSketch(4, 4096);
        for (String sym : stream) {
            exact.merge(sym, 1, Integer::sum);
            cms.add(sym);
        }
        for (Map.Entry<String, Integer> e : exact.entrySet()) {
            assertTrue(cms.estimate(e.getKey()) >= e.getValue(), "one-sided error: never below true");
        }
        assertTrue(cms.estimate("T0007") >= 1);
        assertEquals(0, cms.estimate("NEVER-SEEN"));
    }

    @Test
    void heavyHittersRanksTheHottestSymbols() {
        HeavyHitters hh = new HeavyHitters(3, 4, 4096);
        for (String sym : SampleApp.marketStream()) hh.add(sym);
        List<HeavyHitters.Entry> top = hh.top();
        assertEquals(3, top.size());
        assertEquals("ES", top.get(0).key);
        assertEquals("NQ", top.get(1).key);
        assertEquals("CL", top.get(2).key);
    }

    @Test
    void windowedAgesABurstOut() {
        WindowedCountMinSketch w = new WindowedCountMinSketch(3, 4, 4096);
        for (int i = 0; i < 500; i++) w.add("ES");
        assertTrue(w.estimate("ES") >= 500);
        w.tick();
        w.tick();
        w.tick();
        assertEquals(0, w.estimate("ES"), "the burst slice rotated out of the window");
    }

    @Test
    void mergeTakesMaxNotSum() {
        CountMinSketch venueA = new CountMinSketch(4, 4096);
        CountMinSketch venueB = new CountMinSketch(4, 4096);
        for (int i = 0; i < 300; i++) venueA.add("ES");
        for (int i = 0; i < 120; i++) venueA.add("NQ");
        for (int i = 0; i < 200; i++) venueB.add("NQ");
        Merge.mergeInto(venueA, venueB);
        assertTrue(venueA.estimate("ES") >= 300);
        int nq = venueA.estimate("NQ");
        assertTrue(nq >= 200 && nq < 320, "expected max(120,200) near 200, got " + nq);
    }
}
