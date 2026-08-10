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
        // Size from the error budget rather than guessing (depth, width):
        // 0.1% of the stream volume, 99.9% of the time.
        CountMinSketch cms = CountMinSketch.withErrorBounds(0.001, 0.999);
        for (int i = 0; i < 1000; i++) {
            cms.add("ESZ5");
        }
        for (long i = 0; i < 50_000; i++) {
            cms.addU64(i);
        }

        int est = cms.estimate("ESZ5");
        assertTrue(est >= 1000);                            // never below the true count
        assertTrue(cms.estimateLowerBound("ESZ5") <= 1000); // and bracketed from below
        assertEquals(51_000L, cms.total());

        // Checkpoint and restore without a serialization dependency.
        byte[] bytes = cms.toBytes();
        assertEquals(est, CountMinSketch.fromBytes(bytes).estimate("ESZ5"));
        // quickstart:end
    }

    @Test
    void governorNeverUnderCountsAndBracketsTheTruth() {
        List<SampleApp.Msg> tape = SampleApp.tape();
        Map<String, Integer> exact = new HashMap<>();
        CountMinSketch rates = CountMinSketch.withErrorBounds(0.001, 0.999);
        for (SampleApp.Msg msg : tape) {
            exact.merge(msg.symbol, 1, Integer::sum);
            rates.add(msg.symbol);
        }
        int margin = rates.errorMargin();
        for (Map.Entry<String, Integer> e : exact.entrySet()) {
            int est = rates.estimate(e.getKey());
            assertTrue(est >= e.getValue(), "a throttle decision may never miss a talker");
            assertTrue(Math.max(0, est - margin) <= e.getValue(), "lower bound holds");
        }
        assertEquals(tape.size(), rates.total());
        assertEquals(0, rates.estimate("NEVER-SEEN"));
    }

    @Test
    void governorStateSurvivesACheckpoint() {
        CountMinSketch rates = CountMinSketch.withErrorBounds(0.001, 0.999);
        for (SampleApp.Msg msg : SampleApp.tape()) rates.add(msg.symbol);
        CountMinSketch restored = CountMinSketch.fromBytes(rates.toBytes());
        assertEquals(rates.total(), restored.total());
        assertEquals(rates.estimate("ESZ5"), restored.estimate("ESZ5"));
        assertEquals(rates.estimate("THIN0007"), restored.estimate("THIN0007"));
    }

    @Test
    void bandwidthRankingDiffersFromMessageRanking() {
        HeavyHitters byBytes = new HeavyHitters(3, 5, 8192);
        HeavyHitters byMsgs = new HeavyHitters(3, 5, 8192);
        for (SampleApp.Msg msg : SampleApp.tape()) {
            byBytes.addN(msg.symbol, msg.bytes);
            byMsgs.add(msg.symbol);
        }
        assertEquals(3, byBytes.top().size());
        assertEquals("ESZ5", byBytes.top().get(0).key);
        assertEquals("ESZ5", byMsgs.top().get(0).key);
        // 1500 messages at 128 bytes beats 900 at 64 by far more than the
        // message ranking suggests, which is why the throttle list weights.
        assertTrue(byBytes.estimate("CLF6") > 3 * byBytes.estimate("ZNH6"));
        assertTrue(byMsgs.estimate("CLF6") < 2 * byMsgs.estimate("ZNH6"));
    }

    @Test
    void burstAgesOutOfTheWindow() {
        WindowedCountMinSketch recent = new WindowedCountMinSketch(3, 5, 8192);
        for (int i = 0; i < 500; i++) recent.add("ESZ5");
        assertTrue(recent.estimate("ESZ5") >= 500);
        for (int second = 0; second < 3; second++) {
            recent.tick();
            for (int i = 0; i < 40; i++) recent.add("ESZ5");
        }
        int settled = recent.estimate("ESZ5");
        assertTrue(settled >= 120, "the quiet traffic is still counted");
        assertTrue(settled <= 400, "the burst left the window: " + settled);
    }

    @Test
    void crossVenueRollupKeepsTheUnionBound() {
        CountMinSketch cme = new CountMinSketch(5, 8192);
        CountMinSketch ice = new CountMinSketch(5, 8192);
        for (int i = 0; i < 300; i++) cme.add("ESZ5");
        for (int i = 0; i < 120; i++) cme.add("CLF6");
        for (int i = 0; i < 200; i++) ice.add("CLF6");
        Merge.mergeInto(cme, ice);
        assertTrue(cme.estimate("ESZ5") >= 300);
        int clf = cme.estimate("CLF6");
        assertTrue(clf >= 320, "union of both legs, got " + clf);
        assertTrue(clf < 360, "and not much above it, got " + clf);
        assertEquals(620L, cme.total());
    }
}
