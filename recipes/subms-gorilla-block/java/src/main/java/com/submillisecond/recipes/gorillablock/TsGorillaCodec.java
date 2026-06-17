package com.submillisecond.recipes.gorillablock;

import com.submillisecond.recipes.ts.TsCodec;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;

/**
 * Plugs the block into {@code subms-ts}'s {@link TsCodec} substrate, so a
 * {@code TsSeries<Double>} serializes to / from Gorilla bytes the same way it
 * does to JSON, and composes under codec wrappers.
 */
public final class TsGorillaCodec implements TsCodec<Double> {

    public TsGorillaCodec() {
    }

    @Override
    public byte[] encode(TsSeries<Double> series) {
        TsGorillaBlock b = TsGorillaBlock.withCapacity(series.size() * 2 + 16);
        for (TsPoint<Double> p : series) {
            b.append(p.ts(), p.value());
        }
        return b.bytes();
    }

    @Override
    public TsSeries<Double> decode(byte[] bytes) {
        TsGorillaBlock block = TsGorillaBlock.fromBytes(bytes);
        TsSeries<Double> s = TsSeries.withCapacity(block.len());
        for (TsPoint<Double> p : block) {
            // The block already enforced non-decreasing ts on the way in;
            // push cannot fail on a well-formed block.
            s.push(p.ts(), p.value());
        }
        return s;
    }

    @Override
    public String format() {
        return "gorilla";
    }
}
