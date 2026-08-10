package com.submillisecond.recipes.hll;

import com.submillisecond.recipes.hll.features.SparseHyperLogLog;
import com.submillisecond.recipes.hll.features.UnionIntersect;
import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

/** Pins the behaviour each section of {@link SampleApp} demonstrates. */
final class SampleAppTest {

    @Test
    void quickstart() {
        // quickstart:begin
        HyperLogLog hll = new HyperLogLog(14);
        for (int i = 0; i < 10_000; i++) {
            hll.add("key" + i);
        }
        double est = hll.estimate();
        assertTrue(est > 9_000.0 && est < 11_000.0, "10k distinct within 10%, got " + est);
        // quickstart:end
    }

    @Test
    void sessionCardinalityScenario() {
        int trueDistinct = 50_000;
        HyperLogLog hll = new HyperLogLog(14);
        for (int i = 0; i < trueDistinct; i++) {
            String sess = String.format("sess-%08x", i);
            for (int b = 0; b <= i % 5; b++) hll.add(sess);
        }
        double err = Math.abs(hll.estimate() - trueDistinct) / trueDistinct;
        assertTrue(err < 0.05, "p=14 within 5%, got " + err);
        assertEquals(16_384, hll.registerCount(), "p=14 is a 16 KB register array");
    }

    @Test
    void finerPrecisionBeatsTheFivePercentEnvelope() {
        int trueDistinct = 100_000;
        HyperLogLog coarse = new HyperLogLog(8);
        HyperLogLog fine = new HyperLogLog(14);
        for (int i = 0; i < trueDistinct; i++) {
            String k = String.format("acct-%08x", i);
            coarse.add(k);
            fine.add(k);
        }
        double fineErr = Math.abs(fine.estimate() - trueDistinct) / trueDistinct;
        assertTrue(fineErr < 0.05, "p=14 within 5%, got " + fineErr);
        double coarseErr = Math.abs(coarse.estimate() - trueDistinct) / trueDistinct;
        assertTrue(coarseErr < 0.30, "p=8 within its wide envelope, got " + coarseErr);
    }

    @Test
    void sparseStaysThinThenPromotes() {
        SparseHyperLogLog thin = new SparseHyperLogLog(14);
        for (int cp = 0; cp < 20; cp++) thin.add("cpty-" + cp);
        assertTrue(thin.isSparse(), "thin name stays sparse");
        assertEquals(20, thin.entryCount(), "one entry per distinct counterparty");

        SparseHyperLogLog hot = new SparseHyperLogLog(8);
        for (int cp = 0; cp < 2_000; cp++) hot.add("cpty-" + cp);
        assertFalse(hot.isSparse(), "busy name promotes to dense");
        double est = hot.estimate();
        assertTrue(est > 1_500.0 && est < 2_500.0, "hot estimate near 2000, got " + est);
    }

    @Test
    void unionAndOverlapAcrossVenues() {
        HyperLogLog venueA = new HyperLogLog(14);
        HyperLogLog venueB = new HyperLogLog(14);
        for (int i = 0; i < 40_000; i++) venueA.add("acct-" + i);
        for (int i = 20_000; i < 60_000; i++) venueB.add("acct-" + i);
        double union = UnionIntersect.estimateUnion(venueA, venueB);
        double inter = UnionIntersect.estimateIntersect(venueA, venueB);
        assertTrue(Math.abs(union - 60_000.0) / 60_000.0 < 0.05, "union within 5%, got " + union);
        assertTrue(Math.abs(inter - 20_000.0) / 20_000.0 < 0.25, "overlap within IE band, got " + inter);
    }
    @Test
    void collectorFanInOverTheWire() {
        // Two venue sketches, shipped as bytes and merged by a collector that
        // never sees an account id - the sample app's closing stage.
        HyperLogLog venueA = new HyperLogLog(14);
        HyperLogLog venueB = new HyperLogLog(14);
        for (long i = 0; i < 30_000; i++) venueA.addLong(i);
        for (long i = 20_000; i < 50_000; i++) venueB.addLong(i);
        byte[][] shipped = {HllCodec.toBytes(venueA), HllCodec.toBytes(venueB)};
        for (byte[] b : shipped) {
            assertEquals(8 + 16_384, b.length,
                "a p=14 sketch is 16392 bytes on the wire whatever it counted");
        }
        HyperLogLog firm = new HyperLogLog(14);
        for (byte[] b : shipped) firm.merge(HllCodec.fromBytes(b));
        double est = firm.estimate();
        assertTrue(Math.abs(est - 50_000.0) / 50_000.0 < 0.05,
            "50k distinct accounts firm-wide, got " + est);
    }

    @Test
    void thinSymbolSketchesStayThinOnTheWire() {
        SparseHyperLogLog thin = new SparseHyperLogLog(14, 2_000);
        for (long cp = 0; cp < 30; cp++) thin.addLong(cp);
        byte[] bytes = HllCodec.toBytes(thin);
        assertTrue(bytes.length < 200,
            "30 counterparties should not cost 16 KB on the wire, got " + bytes.length);
        assertEquals(thin.estimate(), HllCodec.sparseFromBytes(bytes).estimate());
    }

    @Test
    void theTapeIsDeterministic() {
        SampleApp.Event[] first = SampleApp.tape();
        SampleApp.Event[] second = SampleApp.tape();
        assertEquals(SampleApp.EVENTS, first.length);
        for (int i = 0; i < first.length; i += 997) {
            assertEquals(first[i], second[i], "printed output must be reproducible");
        }
    }

    @Test
    void everyStageRunsCleanEndToEnd() {
        SampleApp.main(new String[0]);
    }
}
