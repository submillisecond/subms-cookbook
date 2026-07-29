package com.submillisecond.recipes.cms;

import com.submillisecond.recipes.cms.features.HeavyHitters;
import com.submillisecond.recipes.cms.features.Merge;
import com.submillisecond.recipes.cms.features.WindowedCountMinSketch;

import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Sample app: a tour of {@code subms-count-min-sketch} over a market-data
 * feed, base API first, then each optional variant. Run:
 * {@code mvn -q compile exec:java -Dexec.mainClass=com.submillisecond.recipes.cms.SampleApp}
 *
 * <p>Scenario: a high-cardinality tape of per-symbol market-data messages,
 * where the hot symbols have to be found in fixed memory - no exact
 * per-symbol counter that grows with the symbol universe.
 *
 * <ul>
 *   <li>base          - per-symbol message-rate estimate, overestimate-only error
 *   <li>heavy-hitters - the hottest symbols themselves (hot-symbol detection)
 *   <li>windowed      - a recent-rate estimate that ages bursts out by slice
 *   <li>merge         - fan-in of per-shard sketches from sharded feed handlers
 * </ul>
 */
public final class SampleApp {

    private SampleApp() {}

    public static void main(String[] args) {
        baseSymbolRate();
        hotSymbolDetection();
        recentRateWindow();
        fanInMerge();
    }

    /**
     * A synthetic tape: four hot symbols dominate, plus a long tail of cold
     * symbols each seen once - the high-cardinality part an exact counter
     * would have to size for.
     */
    static List<String> marketStream() {
        List<String> stream = new ArrayList<>();
        String[] hot = {"ES", "NQ", "CL", "ZN"};
        int[] hits = {5000, 3000, 1500, 900};
        for (int h = 0; h < hot.length; h++) {
            for (int i = 0; i < hits[h]; i++) stream.add(hot[h]);
        }
        for (int i = 0; i < 4000; i++) stream.add(String.format("T%04d", i));
        return stream;
    }

    /**
     * Base API: estimate how many messages each symbol sent, in fixed memory.
     * The Count-Min guarantee is one-sided - the estimate is always >= the
     * true count, never below - so a rate threshold built on it never misses
     * a genuinely hot symbol; the only error is a bounded over-count.
     */
    static void baseSymbolRate() {
        System.out.println("== base: per-symbol message-rate estimate ==");
        List<String> stream = marketStream();

        Map<String, Integer> exact = new HashMap<>();
        CountMinSketch cms = new CountMinSketch(4, 4096);
        for (String sym : stream) {
            exact.merge(sym, 1, Integer::sum);
            cms.add(sym);
        }
        System.out.println("  " + stream.size() + " messages, " + exact.size()
            + " distinct symbols, into a " + cms.depth() + "x" + cms.width() + " sketch");

        int worstOver = 0;
        for (Map.Entry<String, Integer> e : exact.entrySet()) {
            int est = cms.estimate(e.getKey());
            if (est < e.getValue()) throw new AssertionError("estimate must never underestimate");
            worstOver = Math.max(worstOver, est - e.getValue());
        }
        System.out.println("  estimate >= true for all " + exact.size()
            + " symbols; worst over-count " + worstOver);

        for (String sym : new String[] {"ES", "NQ", "CL", "ZN"}) {
            System.out.println("  " + sym + ": est " + cms.estimate(sym) + " (true " + exact.get(sym) + ")");
        }
        String cold = "T0007";
        System.out.println("  cold " + cold + ": est " + cms.estimate(cold) + " (true " + exact.get(cold) + ")");
        if (cms.estimate(cold) < exact.get(cold)) throw new AssertionError("cold key not under-counted");
    }

    /**
     * heavy-hitters: the base sketch scores a symbol you name, but it cannot
     * list the hottest symbols on its own - the cell layout is lossy by
     * design. HeavyHitters keeps a top-K side index refreshed on every add.
     */
    static void hotSymbolDetection() {
        System.out.println("\n== heavy-hitters: the hottest symbols ==");
        HeavyHitters hh = new HeavyHitters(3, 4, 4096);
        for (String sym : marketStream()) hh.add(sym);
        System.out.println("  top " + hh.k() + " symbols:");
        for (HeavyHitters.Entry entry : hh.top()) {
            System.out.println("    " + entry.key + " ~" + entry.estimate);
        }
        List<HeavyHitters.Entry> top = hh.top();
        if (top.size() != 3) throw new AssertionError("exactly K tracked");
        if (!top.get(0).key.equals("ES")) throw new AssertionError("busiest symbol leads");
        if (!top.get(1).key.equals("NQ")) throw new AssertionError("second-busiest is NQ");
        if (!top.get(2).key.equals("CL")) throw new AssertionError("third-busiest is CL");
    }

    /**
     * windowed: an all-time counter never forgets. A ring of sub-sketches
     * ages old bursts out - tick() rotates the ring and clears the slice that
     * just rolled over, so the estimate reflects only the recent window.
     */
    static void recentRateWindow() {
        System.out.println("\n== windowed: recent message rate ==");
        WindowedCountMinSketch w = new WindowedCountMinSketch(3, 4, 4096);
        for (int i = 0; i < 500; i++) w.add("ES");
        System.out.println("  ES in-window right after the burst: " + w.estimate("ES"));
        if (w.estimate("ES") < 500) throw new AssertionError("burst visible in-window");

        w.tick();
        w.tick();
        w.tick();
        System.out.println("  ES in-window after the slice rotated out: " + w.estimate("ES"));
        if (w.estimate("ES") != 0) throw new AssertionError("the burst aged out of the window");
    }

    /**
     * merge: shard the tape across feed handlers, each accumulating its own
     * lock-free sketch of identical shape, then fold at the join. The combiner
     * is element-wise MAX, not sum - each shard already absorbed its own
     * conservative-update damping, so addition would double-count a symbol
     * that traded on more than one venue.
     */
    static void fanInMerge() {
        System.out.println("\n== merge: fan-in of per-shard sketches ==");
        CountMinSketch venueA = new CountMinSketch(4, 4096);
        CountMinSketch venueB = new CountMinSketch(4, 4096);
        for (int i = 0; i < 300; i++) venueA.add("ES");
        for (int i = 0; i < 120; i++) venueA.add("NQ");
        for (int i = 0; i < 200; i++) venueB.add("NQ");

        Merge.mergeInto(venueA, venueB);
        int es = venueA.estimate("ES");
        int nq = venueA.estimate("NQ");
        System.out.println("  merged ES: " + es);
        System.out.println("  merged NQ: " + nq + "  (max of 120 and 200, not their sum)");
        if (es < 300) throw new AssertionError("ES only traded on venue A");
        if (nq < 200 || nq >= 320) throw new AssertionError("max-merge keeps NQ near 200, not 320");
    }
}
