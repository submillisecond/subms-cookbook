package com.submillisecond.recipes.zonemap;

import java.util.ArrayList;
import java.util.List;
import java.util.Optional;

import com.submillisecond.recipes.gorillablock.TsBlockStats;
import com.submillisecond.recipes.gorillablock.TsGorillaBlock;

/**
 * A per-block min/max/count index that lets a query planner skip blocks it
 * cannot touch. For a predicate like {@code (ts in [t1, t2] AND value > X)}, a
 * block whose {@code [tsMin, tsMax]} misses the window or whose
 * {@code [valueMin, valueMax]} cannot satisfy the value test is pruned without
 * reading its body - the predicate-pushdown step that lets a TSDB scan
 * terabytes at memory-bandwidth rates.
 *
 * <p>Behaviour is parity-equivalent to the Rust {@code subms-zone-map} crate.
 *
 * <pre>
 *   TsZoneMap z = new TsZoneMap();
 *   TsGorillaBlock b = new TsGorillaBlock();
 *   for (int i = 0; i &lt; 100; i++) { b.append(1_000 + i, i); }
 *   z.observe(7, b);
 *
 *   // window misses the block -&gt; pruned
 *   assert z.candidates(50_000, 60_000).length == 0;
 *   // value predicate value &gt; 200 cannot hold (max is 99) -&gt; pruned
 *   TsValuePredicate pred = TsValuePredicate.of(TsValueOp.GT, 200.0);
 *   assert z.candidates(1_000, 1_099, Optional.of(pred)).length == 0;
 *   // satisfiable -&gt; candidate
 *   assert z.candidates(1_000, 1_099)[0] == 7;
 * </pre>
 */
public final class TsZoneMap {

    private final List<TsZone> zones;

    public TsZoneMap() {
        this.zones = new ArrayList<>();
    }

    public TsZoneMap(int capacity) {
        this.zones = new ArrayList<>(capacity);
    }

    public static TsZoneMap withCapacity(int capacity) {
        return new TsZoneMap(capacity);
    }

    /**
     * Record the zone for a Gorilla block (reads only its {@code stats}, never
     * the body). Empty blocks are skipped.
     */
    public void observe(long blockId, TsGorillaBlock block) {
        if (block.isEmpty()) {
            return;
        }
        TsBlockStats s = block.stats();
        zones.add(new TsZone(blockId, s.tsMin(), s.tsMax(), s.valueMin(), s.valueMax(), s.count()));
    }

    /**
     * Record a zone directly (when the block lives elsewhere or stats are
     * already known).
     */
    public void observeZone(TsZone zone) {
        zones.add(zone);
    }

    /** Block ids whose time range overlaps {@code [tsLo, tsHi]} (inclusive). */
    public long[] candidates(long tsLo, long tsHi) {
        return candidates(tsLo, tsHi, Optional.empty());
    }

    /**
     * Block ids whose {@code [tsMin, tsMax]} overlaps {@code [tsLo, tsHi]} and,
     * if a value predicate is given, whose value range could satisfy it. Order
     * preserved (observation order).
     */
    public long[] candidates(long tsLo, long tsHi, Optional<TsValuePredicate> valuePred) {
        if (tsLo > tsHi) {
            return new long[0];
        }
        TsValuePredicate pred = valuePred.orElse(null);
        long[] buf = new long[zones.size()];
        int n = 0;
        for (TsZone z : zones) {
            if (z.tsMax() < tsLo || z.tsMin() > tsHi) {
                continue;
            }
            if (pred != null && !pred.satisfiable(z.valueMin(), z.valueMax())) {
                continue;
            }
            buf[n++] = z.blockId();
        }
        if (n == buf.length) {
            return buf;
        }
        long[] out = new long[n];
        System.arraycopy(buf, 0, out, 0, n);
        return out;
    }

    public List<TsZone> zones() {
        return List.copyOf(zones);
    }

    public int len() {
        return zones.size();
    }

    public boolean isEmpty() {
        return zones.isEmpty();
    }

    public void clear() {
        zones.clear();
    }
}
