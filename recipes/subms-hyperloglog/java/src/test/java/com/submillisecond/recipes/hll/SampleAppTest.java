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
}
