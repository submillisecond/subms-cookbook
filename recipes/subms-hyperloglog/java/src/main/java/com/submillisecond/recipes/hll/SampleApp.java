package com.submillisecond.recipes.hll;

import com.submillisecond.recipes.hll.features.SparseHyperLogLog;
import com.submillisecond.recipes.hll.features.UnionIntersect;

/**
 * Sample app: a tour of {@code subms-hyperloglog}, base API first, then each
 * optional feature. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.hll.SampleApp}
 *
 * <ul>
 *   <li>base - distinct client-session count in a trading gateway, plus the
 *       accuracy-vs-memory tradeoff across precisions
 *   <li>sparse - one distinct-counterparty sketch per instrument, kept cheap for
 *       the thin long tail until a name gets busy
 *   <li>union-intersect - distinct account reach and overlap across two venues,
 *       without shipping raw id lists between them
 * </ul>
 */
public final class SampleApp {

    public static void main(String[] args) {
        baseSessionCardinality();
        accuracyVsMemory();
        sparsePerInstrument();
        crossVenueReach();
    }

    /** Base API: distinct active sessions in a gateway firehose, from a fixed 16 KB array. */
    static void baseSessionCardinality() {
        System.out.println("== base: distinct client sessions in a trading gateway ==");
        int trueDistinct = 50_000;
        HyperLogLog hll = new HyperLogLog(14);
        long events = 0;
        for (int i = 0; i < trueDistinct; i++) {
            String sess = String.format("sess-%08x", i);
            for (int b = 0; b <= i % 5; b++) {
                hll.add(sess);
                events++;
            }
        }
        double est = hll.estimate();
        double err = Math.abs(est - trueDistinct) / trueDistinct;
        System.out.println("  stream:   " + events + " messages, " + trueDistinct + " distinct sessions");
        System.out.printf("  estimate: %.0f  (%.2f%% error)%n", est, err * 100.0);
        System.out.println("  state:    " + hll.registerCount() + " registers = ~"
            + (hll.registerCount() / 1024) + " KB, fixed no matter the stream length");
        if (err >= 0.05) throw new AssertionError("p=14 estimate within 5%, got " + err);
    }

    /** Base API: the accuracy-vs-memory dial. Smaller state, larger error. */
    static void accuracyVsMemory() {
        System.out.println("\n== base: the accuracy-vs-memory tradeoff ==");
        int trueDistinct = 100_000;
        double bestErr = Double.POSITIVE_INFINITY;
        for (int p : new int[] {8, 11, 14}) {
            HyperLogLog hll = new HyperLogLog(p);
            for (int i = 0; i < trueDistinct; i++) {
                hll.add(String.format("acct-%08x", i));
            }
            double est = hll.estimate();
            double err = Math.abs(est - trueDistinct) / trueDistinct;
            double kb = hll.registerCount() / 1024.0;
            System.out.printf("  p=%-2d m=%-6d ~%5.1f KB  estimate %9.0f  err %.2f%%%n",
                p, hll.registerCount(), kb, est, err * 100.0);
            bestErr = Math.min(bestErr, err);
        }
        if (bestErr >= 0.05) throw new AssertionError("best precision within 5%, got " + bestErr);
    }

    /** sparse: one distinct-counterparty sketch per instrument, cheap for the thin tail. */
    static void sparsePerInstrument() {
        System.out.println("\n== sparse: distinct counterparties per instrument (long tail) ==");
        SparseHyperLogLog thin = new SparseHyperLogLog(14);
        for (int cp = 0; cp < 20; cp++) thin.add("cpty-" + cp);
        System.out.println("  thin name: 20 counterparties -> sparse=" + thin.isSparse()
            + ", " + thin.entryCount() + " entries held (no 16 KB array)");
        if (!thin.isSparse()) throw new AssertionError("a thinly-traded instrument stays sparse");

        SparseHyperLogLog hot = new SparseHyperLogLog(8);
        for (int cp = 0; cp < 2_000; cp++) hot.add("cpty-" + cp);
        double est = hot.estimate();
        System.out.printf("  hot name:  2000 counterparties -> promoted to dense=%b, estimate %.0f%n",
            !hot.isSparse(), est);
        if (hot.isSparse()) throw new AssertionError("a busy instrument promotes to dense");
        if (est <= 1_500.0 || est >= 2_500.0) throw new AssertionError("hot estimate near 2000, got " + est);
    }

    /** union-intersect: total account reach and overlap across two venues. */
    static void crossVenueReach() {
        System.out.println("\n== union-intersect: distinct accounts across two venues ==");
        HyperLogLog venueA = new HyperLogLog(14);
        HyperLogLog venueB = new HyperLogLog(14);
        for (int i = 0; i < 40_000; i++) venueA.add("acct-" + i);
        for (int i = 20_000; i < 60_000; i++) venueB.add("acct-" + i);
        double union = UnionIntersect.estimateUnion(venueA, venueB);
        double inter = UnionIntersect.estimateIntersect(venueA, venueB);
        System.out.println("  venue A: 40k accounts, venue B: 40k accounts, 20k shared");
        System.out.printf("  total reach (union):   %7.0f  (true 60000)%n", union);
        System.out.printf("  both venues (overlap): %7.0f  (true 20000)%n", inter);
        if (Math.abs(union - 60_000.0) / 60_000.0 >= 0.05) throw new AssertionError("union within 5%, got " + union);
        if (Math.abs(inter - 20_000.0) / 20_000.0 >= 0.25) throw new AssertionError("overlap within IE band, got " + inter);
    }
}
