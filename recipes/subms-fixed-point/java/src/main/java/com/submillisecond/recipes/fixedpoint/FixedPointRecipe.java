package com.submillisecond.recipes.fixedpoint;

import com.submillisecond.perf.SubMsBenchParams;
import com.submillisecond.perf.SubMsPerfHarness;
import com.submillisecond.perf.SubMsRecipe;
import com.submillisecond.perf.SubMsStageKind;
import com.submillisecond.perf.SubMsTimer;

/** Stages: {@code add}, {@code mul}, {@code div}, {@code round_tick}. */
public final class FixedPointRecipe implements SubMsRecipe {

    /** Keeps the accumulated result live. */
    static volatile long sink;

    /** Deterministic operand generator, the same LCG as the Rust port. */
    static final class Lcg {
        private long state;

        Lcg(long seed) {
            state = seed * 0x9E3779B97F4A7C15L + 1;
        }

        long next() {
            state = state * 6364136223846793005L + 1442695040888963407L;
            return state >>> 11;
        }

        /** A price within +/- 5% of 100.00 on a 0.01 grid. */
        Fixed price() {
            long cents = 9_500 + Long.remainderUnsigned(next(), 1_001);
            return Fixed.fromRaw(cents * 10_000_000L);
        }

        /** A positive quantity of 1..=5000 lots. */
        Fixed qty() {
            return Fixed.fromInt(1 + Long.remainderUnsigned(next(), 5_000));
        }

        /** A price a few billionths off the grid, for the rounding stage. */
        Fixed offGrid() {
            Fixed p = price();
            return Fixed.fromRaw(p.raw() + Long.remainderUnsigned(next(), 9_999_999));
        }
    }

    @FunctionalInterface
    private interface Op {
        long at(int k);
    }

    private static long stage(SubMsPerfHarness h, String name, int entries, Op op) {
        SubMsPerfHarness.Stage s = h.stage(name, entries).withKind(SubMsStageKind.HOT_PATH);
        long acc = 0;
        for (int k = 0; k < entries; k++) {
            long t0 = SubMsTimer.nanosNow();
            long r = op.at(k);
            long ns = SubMsTimer.nanosNow() - t0;
            acc ^= r;
            s.record(ns);
        }
        return acc;
    }

    @Override
    public String name() {
        return "fixed-point";
    }

    @Override
    public void run(SubMsPerfHarness h, SubMsBenchParams params) {
        int entries = params.entries();
        int warmup = params.warmup();
        Lcg rng = new Lcg(params.seed());
        int n = Math.max(entries, 1);
        long[] prices = new long[n];
        long[] qtys = new long[n];
        long[] dirty = new long[n];
        for (int i = 0; i < n; i++) {
            prices[i] = rng.price().raw();
        }
        for (int i = 0; i < n; i++) {
            qtys[i] = rng.qty().raw();
        }
        for (int i = 0; i < n; i++) {
            dirty[i] = rng.offGrid().raw();
        }
        Fixed tick = Fixed.fromRaw(10_000_000L);
        Rounding mode = Rounding.HALF_EVEN;
        long acc = 0;

        for (int i = 0; i < warmup; i++) {
            int k = i % n;
            Fixed p = Fixed.fromRaw(prices[k]);
            Fixed q = Fixed.fromRaw(qtys[k]);
            acc ^= p.add(q).raw();
            acc ^= p.mul(q, mode).raw();
            acc ^= p.div(q, mode).raw();
            acc ^= Fixed.fromRaw(dirty[k]).roundToTick(tick, mode).raw();
        }

        acc ^= stage(h, "add", entries, k -> Fixed.fromRaw(prices[k]).add(Fixed.fromRaw(qtys[k])).raw());
        acc ^= stage(h, "mul", entries, k -> Fixed.fromRaw(prices[k]).mul(Fixed.fromRaw(qtys[k]), mode).raw());
        acc ^= stage(h, "div", entries, k -> Fixed.fromRaw(prices[k]).div(Fixed.fromRaw(qtys[k]), mode).raw());
        acc ^= stage(h, "round_tick", entries, k -> Fixed.fromRaw(dirty[k]).roundToTick(tick, mode).raw());

        sink = acc;
        h.meta("decimals", "9");
        h.meta("rounding", mode.name());
        h.meta("tick", "0.01");
    }
}
