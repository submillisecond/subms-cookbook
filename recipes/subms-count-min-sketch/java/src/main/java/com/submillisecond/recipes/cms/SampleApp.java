package com.submillisecond.recipes.cms;

import com.submillisecond.recipes.cms.features.HeavyHitters;
import com.submillisecond.recipes.cms.features.Merge;
import com.submillisecond.recipes.cms.features.WindowedCountMinSketch;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * Sample app: a per-symbol message-rate governor for a market-data gateway.
 *
 * <p>The gateway sees a high-cardinality tape - a handful of hot futures plus
 * a long tail of thinly-traded symbols - and has to answer three questions in
 * fixed memory: how fast is this symbol talking, which symbols are the
 * loudest, and has anything burst in the last few seconds. An exact per-symbol
 * counter answers all three and grows with the symbol universe, which is the
 * thing the gateway cannot afford.
 *
 * <p>Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.cms.SampleApp}
 *
 * <ul>
 *   <li>base          - the governor itself: sizing, rate verdicts, checkpointing
 *   <li>heavy-hitters - the throttle list, ranked by bytes rather than messages
 *   <li>windowed      - burst detection that ages out
 *   <li>merge         - folding per-venue shards at the join
 * </ul>
 */
public final class SampleApp {

    private SampleApp() {}

    public static void main(String[] args) {
        List<Msg> tape = tape();
        RateGovernor gov = RateGovernor.ingest(tape);
        gov.report(tape);
        gov.checkpoint();
        throttleList(tape);
        burstGovernor();
        crossVenueRollup();
    }

    /** One tape entry: a symbol and the size of the message that carried it. */
    static final class Msg {
        final String symbol;
        final int bytes;

        Msg(String symbol, int bytes) {
            this.symbol = symbol;
            this.bytes = bytes;
        }
    }

    /**
     * A deterministic replay of one gateway second. Four hot futures dominate
     * the message count; the 4000 thin symbols behind them are what an exact
     * counter would have to size for.
     */
    static List<Msg> tape() {
        List<Msg> tape = new ArrayList<>();
        String[] hot = {"ESZ5", "NQZ5", "CLF6", "ZNH6"};
        int[] msgs = {5000, 3000, 1500, 900};
        int[] sizes = {96, 96, 128, 64};
        for (int h = 0; h < hot.length; h++) {
            for (int i = 0; i < msgs[h]; i++) tape.add(new Msg(hot[h], sizes[h]));
        }
        for (int i = 0; i < 4000; i++) {
            tape.add(new Msg(String.format(Locale.ROOT, "THIN%04d", i), 64));
        }
        return tape;
    }

    /** The governor: one sketch, one threshold, one verdict per symbol. */
    static final class RateGovernor {

        private final CountMinSketch rates;
        private final int limit;

        private RateGovernor(CountMinSketch rates, int limit) {
            this.rates = rates;
            this.limit = limit;
        }

        /**
         * Size from the error budget rather than from a guessed (depth, width):
         * tolerate an over-count of 0.1% of gateway volume, 99.9% of the time.
         */
        static RateGovernor ingest(List<Msg> tape) {
            CountMinSketch rates = CountMinSketch.withErrorBounds(0.001, 0.999);
            for (Msg msg : tape) rates.add(msg.symbol);
            return new RateGovernor(rates, 2000);
        }

        /**
         * The estimate is an upper bound, so a symbol under the limit is under
         * it for certain. That is the direction a governor wants to be wrong in.
         */
        String verdict(String symbol) {
            return rates.estimate(symbol) > limit ? "THROTTLE" : "pass";
        }

        void report(List<Msg> tape) {
            System.out.println("== governor: per-symbol message rates ==");
            Map<String, Integer> exact = new HashMap<>();
            for (Msg msg : tape) exact.merge(msg.symbol, 1, Integer::sum);
            System.out.println("  " + tape.size() + " messages, " + exact.size() + " distinct symbols");
            System.out.println(String.format(Locale.ROOT,
                "  sketch %dx%d = %d KiB, error <= %.4f%% of volume at %.3f confidence",
                rates.depth(), rates.width(), rates.heapBytes() / 1024,
                rates.relativeError() * 100.0, rates.confidence()));
            System.out.println(String.format(Locale.ROOT,
                "  volume %d, over-count budget %d msgs, cells touched %.1f%%",
                rates.total(), rates.errorMargin(), rates.occupancy() * 100.0));

            int worstOver = 0;
            for (Map.Entry<String, Integer> e : exact.entrySet()) {
                int est = rates.estimate(e.getKey());
                if (est < e.getValue()) {
                    throw new AssertionError("estimate must never fall below the true count");
                }
                worstOver = Math.max(worstOver, est - e.getValue());
            }
            System.out.println("  worst over-count across every symbol: " + worstOver);

            for (String symbol : new String[] {"ESZ5", "NQZ5", "CLF6", "ZNH6", "THIN0007"}) {
                System.out.println(String.format(Locale.ROOT,
                    "  %-9s %d..%d msgs (true %d) -> %s",
                    symbol, rates.estimateLowerBound(symbol), rates.estimate(symbol),
                    exact.get(symbol), verdict(symbol)));
            }
        }

        /**
         * The gateway restarts often. A snapshot is a plain byte array, so the
         * governor comes back with its rate history instead of a cold sketch.
         */
        void checkpoint() {
            System.out.println("\n== checkpoint: survive a gateway restart ==");
            byte[] bytes = rates.toBytes();
            CountMinSketch restored = CountMinSketch.fromBytes(bytes);
            System.out.println("  " + bytes.length + " bytes on the wire; restored "
                + restored.depth() + "x" + restored.width() + ", volume " + restored.total());
            System.out.println("  ESZ5 before " + rates.estimate("ESZ5")
                + " / after " + restored.estimate("ESZ5"));
            if (restored.estimate("ESZ5") != rates.estimate("ESZ5")) {
                throw new AssertionError("snapshot round trip changed an estimate");
            }
        }
    }

    /**
     * heavy-hitters: the base sketch scores a symbol you name, but it cannot
     * list the loudest symbols on its own - the cell layout is lossy by design.
     * The throttle list needs the ranking, and it ranks on BYTES rather than
     * messages, because a 128-byte depth update costs the gateway twice what a
     * 64-byte trade print does. Weighted add is the same call with the size.
     */
    static void throttleList(List<Msg> tape) {
        System.out.println("\n== throttle list: loudest symbols by bandwidth ==");
        HeavyHitters byBytes = new HeavyHitters(3, 5, 8192);
        HeavyHitters byMsgs = new HeavyHitters(3, 5, 8192);
        for (Msg msg : tape) {
            byBytes.addN(msg.symbol, msg.bytes);
            byMsgs.add(msg.symbol);
        }
        System.out.println("  " + byBytes.total() + " bytes ranked, top " + byBytes.k() + " held:");
        for (HeavyHitters.Entry entry : byBytes.top()) {
            System.out.println(String.format(Locale.ROOT, "    %-9s ~%d bytes", entry.key, entry.estimate));
        }
        if (byBytes.top().size() != 3) throw new AssertionError("exactly K tracked");
        if (!byBytes.top().get(0).key.equals("ESZ5")) throw new AssertionError("busiest symbol leads");

        // CLF6 carries 128-byte depth updates against ZNH6's 64-byte prints, so
        // weighting by size roughly doubles the gap the message count reports.
        double bytesGap = (double) byBytes.estimate("CLF6") / byBytes.estimate("ZNH6");
        double msgGap = (double) byMsgs.estimate("CLF6") / byMsgs.estimate("ZNH6");
        System.out.println(String.format(Locale.ROOT,
            "  CLF6 over ZNH6: %.2fx by bytes, %.2fx by messages", bytesGap, msgGap));
    }

    /**
     * windowed: an all-time counter never forgets, so a burst that ended a
     * minute ago still trips the limit. A ring of sub-sketches ages it out -
     * the caller owns the clock by choosing when to tick, one tick per second
     * here.
     */
    static void burstGovernor() {
        System.out.println("\n== burst governor: a 3-second window ==");
        WindowedCountMinSketch recent = new WindowedCountMinSketch(3, 5, 8192);
        int limit = 400;

        for (int i = 0; i < 500; i++) recent.add("ESZ5");
        System.out.println("  after a 500-message burst: in-window " + recent.estimate("ESZ5")
            + " (limit " + limit + ") -> " + (recent.estimate("ESZ5") > limit ? "THROTTLE" : "pass"));

        for (int second = 1; second <= 3; second++) {
            recent.tick();
            for (int i = 0; i < 40; i++) recent.add("ESZ5");
            System.out.println("  +" + second + "s quiet trading: in-window " + recent.estimate("ESZ5"));
        }
        int settled = recent.estimate("ESZ5");
        System.out.println("  burst aged out, " + settled + " msgs in window -> pass");
        if (settled > limit) throw new AssertionError("the burst left the window");
        System.out.println("  window costs " + recent.heapBytes() / 1024 + " KiB, "
            + recent.slices() + "x a single sketch");
    }

    /**
     * merge: the gateway runs one handler per venue, each with its own sketch
     * and no shared state on the write path. The rollup happens once, at the
     * join. Cells are summed because a symbol trades on both venues, and
     * summing is what keeps the merged estimate above the true cross-venue
     * count.
     */
    static void crossVenueRollup() {
        System.out.println("\n== rollup: two venue shards folded at the join ==");
        CountMinSketch cme = new CountMinSketch(5, 8192);
        CountMinSketch ice = new CountMinSketch(5, 8192);
        for (int i = 0; i < 300; i++) cme.add("ESZ5");
        for (int i = 0; i < 120; i++) cme.add("CLF6");
        for (int i = 0; i < 200; i++) ice.add("CLF6");

        Merge.mergeInto(cme, ice);
        System.out.println("  ESZ5 (CME only):     " + cme.estimate("ESZ5"));
        System.out.println("  CLF6 (both venues):  " + cme.estimate("CLF6") + " (120 + 200)");
        System.out.println("  rolled-up volume:    " + cme.total());
        if (cme.estimate("ESZ5") < 300) throw new AssertionError("ESZ5 only traded on CME");
        if (cme.estimate("CLF6") < 320) throw new AssertionError("the union count, not the larger leg");
    }
}
