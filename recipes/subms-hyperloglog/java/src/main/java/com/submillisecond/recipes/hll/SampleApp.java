package com.submillisecond.recipes.hll;

import com.submillisecond.recipes.hll.features.SparseHyperLogLog;
import com.submillisecond.recipes.hll.features.UnionIntersect;

/**
 * Sample app: distinct-count telemetry for a two-venue trading gateway.
 *
 * <p>One deterministic tape of order events drives the whole thing. The
 * gateway counts distinct sessions per window, risk keeps a per-symbol
 * counterparty sketch, and each venue ships its account sketch to a collector
 * that merges them into a firm-wide reach number without ever seeing an
 * account id.
 *
 * <pre>
 *   mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.hll.SampleApp
 * </pre>
 */
public final class SampleApp {

    static final String[] SYMBOLS = {"AAPL", "MSFT", "NVDA", "TSLA", "AMZN", "META", "GOOG", "NFLX"};
    /**
     * Distinct counterparties trading each symbol. Two liquid names and a long
     * tail, which is what makes a per-symbol dense array wasteful.
     */
    static final long[] COUNTERPARTY_POOL = {30_000, 30_000, 900, 700, 60, 40, 15, 9};
    static final int EVENTS = 200_000;

    record Event(int venue, int symbol, long account, long counterparty, long session) {}

    /** Seeded so the printed report is the same on every run and on both ports. */
    static final class Lcg {
        private long s;

        Lcg(long seed) { this.s = seed; }

        long next() {
            s = s * 6364136223846793005L + 1442695040888963407L;
            return s >>> 11;
        }
    }

    /**
     * The tape. Venue 0 and venue 1 draw accounts from overlapping ranges, and
     * symbol popularity follows a sharp head-and-tail split - the two facts
     * every stage below is trying to measure.
     */
    static Event[] tape() {
        Lcg rng = new Lcg(0x5eedL);
        Event[] out = new Event[EVENTS];
        for (int i = 0; i < EVENTS; i++) {
            int venue = (int) (rng.next() % 2);
            long r = rng.next() % 100;
            int symbol = r < 66 ? (int) (r % 2) : 2 + (int) (rng.next() % 6);
            long account = venue == 0 ? rng.next() % 30_000 : 20_000 + rng.next() % 30_000;
            long counterparty = ((long) symbol << 32) | (rng.next() % COUNTERPARTY_POOL[symbol]);
            out[i] = new Event(venue, symbol, account, counterparty, rng.next() % 40_000);
        }
        return out;
    }

    public static void main(String[] args) {
        Event[] tape = tape();
        System.out.println("tape: " + tape.length + " order events across 2 venues, 8 symbols\n");

        gatewaySessions(tape);
        sizeFromAnErrorBudget();
        perSymbolCounterparties(tape);
        crossVenueOverlap(tape);
        collectorFanIn(tape);
    }

    /**
     * The hot path. Every message on the gateway records its session id, and
     * the answer costs 16 KB whether the window held 40 thousand sessions or
     * 40 million. {@code addLong} skips rendering the id to a string first.
     */
    static void gatewaySessions(Event[] tape) {
        System.out.println("== gateway: distinct sessions this window ==");
        HyperLogLog hll = new HyperLogLog(14);
        long firstSightings = 0;
        for (Event e : tape) {
            if (hll.addLong(e.session())) firstSightings++;
        }
        double est = hll.estimate();
        System.out.printf("  %d messages -> %.0f distinct sessions%n", tape.length, est);
        System.out.printf("  %d registers advanced, %d bytes of state, +/- %.2f%% standard error%n",
            firstSightings, hll.stateBytes(), hll.standardError() * 100.0);
        if (est <= 30_000.0 || est >= 50_000.0) {
            throw new AssertionError("~40k sessions, got " + est);
        }
    }

    /**
     * Sizing runs the other way round in production: you are handed an error
     * budget, not a precision. {@code precisionForStandardError} turns the
     * budget into the cheapest register array that meets it.
     */
    static void sizeFromAnErrorBudget() {
        System.out.println("\n== sizing: error budget -> byte budget ==");
        for (double budget : new double[] {0.05, 0.02, 0.01, 0.005}) {
            int p = HyperLogLog.precisionForStandardError(budget);
            HyperLogLog hll = new HyperLogLog(p);
            System.out.printf("  budget %5.1f%%  ->  p=%-2d %6d bytes, actual %.5f%%%n",
                budget * 100.0, p, hll.stateBytes(), hll.standardError() * 100.0);
        }
    }

    /**
     * Risk wants distinct counterparties per symbol. Most of the book is thin,
     * so allocating 16 KB per name would cost 128 KB here and gigabytes across
     * a real universe. The sparse encoding pays only for registers actually
     * touched, and promotes the two busy names once they earn it.
     */
    static void perSymbolCounterparties(Event[] tape) {
        System.out.println("\n== risk: distinct counterparties per symbol ==");
        SparseHyperLogLog[] books = new SparseHyperLogLog[SYMBOLS.length];
        for (int i = 0; i < books.length; i++) {
            books[i] = new SparseHyperLogLog(14, 2_000);
        }
        for (Event e : tape) {
            books[e.symbol()].addLong(e.counterparty());
        }

        int sparseBytes = 0;
        for (int i = 0; i < books.length; i++) {
            SparseHyperLogLog b = books[i];
            System.out.printf("  %-5s %7.0f counterparties  %6d bytes  %s%n",
                SYMBOLS[i], b.estimate(), b.stateBytes(), b.isSparse() ? "sparse" : "dense");
            sparseBytes += b.stateBytes();
        }
        int denseBytes = SYMBOLS.length * 16_384;
        System.out.println("  total " + sparseBytes + " bytes against " + denseBytes
            + " if every name held a dense array");
        if (sparseBytes >= denseBytes) {
            throw new AssertionError("sparse must win on the long tail");
        }
    }

    /**
     * How many accounts trade on both venues? Inclusion-exclusion answers it
     * from two sketches. The error bound is printed next to the answer because
     * it scales with |A| + |B| rather than with the overlap, and an overlap
     * smaller than its own bound is not a number to act on.
     */
    static void crossVenueOverlap(Event[] tape) {
        System.out.println("\n== venues: account reach and overlap ==");
        HyperLogLog a = new HyperLogLog(14);
        HyperLogLog b = new HyperLogLog(14);
        for (Event e : tape) {
            if (e.venue() == 0) a.addLong(e.account());
            else b.addLong(e.account());
        }
        double union = UnionIntersect.estimateUnion(a, b);
        double inter = UnionIntersect.estimateIntersect(a, b);
        double bound = UnionIntersect.intersectErrorBound(a, b);
        System.out.printf("  venue 0: %7.0f accounts%n", a.estimate());
        System.out.printf("  venue 1: %7.0f accounts%n", b.estimate());
        System.out.printf("  reach:   %7.0f (true 50000)%n", union);
        System.out.printf("  both:    %7.0f (true 10000) +/- %.0f%n", inter, bound);
        if (Math.abs(union - 50_000.0) / 50_000.0 >= 0.05) {
            throw new AssertionError("reach within 5%, got " + union);
        }
        if (inter <= 0.0) {
            throw new AssertionError("a 10k overlap must survive the subtraction");
        }
    }

    /**
     * Each venue ships its sketch, not its account list. The collector decodes
     * and merges, and the firm-wide number falls out of 16 KB per venue
     * instead of a million ids on the wire.
     */
    static void collectorFanIn(Event[] tape) {
        System.out.println("\n== collector: merge shipped sketches into a firm-wide reach ==");
        HyperLogLog[] perVenue = {new HyperLogLog(14), new HyperLogLog(14)};
        for (Event e : tape) {
            perVenue[e.venue()].addLong(e.account());
        }
        byte[][] shipped = new byte[perVenue.length][];
        int onWire = 0;
        for (int i = 0; i < perVenue.length; i++) {
            shipped[i] = HllCodec.toBytes(perVenue[i]);
            onWire += shipped[i].length;
        }

        HyperLogLog firm = new HyperLogLog(14);
        for (byte[] bytes : shipped) {
            firm.merge(HllCodec.fromBytes(bytes));
        }
        System.out.println("  " + shipped.length + " sketches on the wire, " + onWire + " bytes total");
        System.out.println("  raw ids would have been ~" + (tape.length * 8L) + " bytes");
        System.out.printf("  firm-wide reach: %.0f (true 50000)%n", firm.estimate());
        if (Math.abs(firm.estimate() - 50_000.0) / 50_000.0 >= 0.05) {
            throw new AssertionError("merged reach within 5%");
        }
    }

    private SampleApp() {}
}
