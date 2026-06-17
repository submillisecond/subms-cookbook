package com.submillisecond.recipes.tsgzip;

import com.submillisecond.recipes.ts.TsCodec;
import com.submillisecond.recipes.ts.TsJsonCodec;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesD;

/**
 * A {@code TsCodec<Double>} view over {@code subms-ts}'s columnar
 * {@link TsJsonCodec} (which speaks the primitive-double {@link TsSeriesD}).
 * It is the inner codec the gzip recipe wraps to form {@code gzip+json},
 * mirroring the Rust recipe's {@code TsGzipCodec::new(TsJsonCodec::new(), ..)}.
 */
public final class JsonInnerCodec implements TsCodec<Double> {

    private final TsJsonCodec json = new TsJsonCodec();

    @Override
    public byte[] encode(TsSeries<Double> series) {
        TsSeriesD d = TsSeriesD.withCapacity(series.size());
        for (TsPoint<Double> p : series) {
            d.push(p.ts(), p.value());
        }
        return json.encode(d);
    }

    @Override
    public TsSeries<Double> decode(byte[] bytes) {
        TsSeriesD d = json.decode(bytes);
        TsSeries<Double> s = TsSeries.withCapacity(d.size());
        for (TsPoint<Double> p : d.toList()) {
            s.push(p.ts(), p.value());
        }
        return s;
    }

    @Override
    public String format() {
        return json.format();
    }
}
