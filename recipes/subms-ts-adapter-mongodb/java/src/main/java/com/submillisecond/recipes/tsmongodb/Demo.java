package com.submillisecond.recipes.tsmongodb;

import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import java.util.List;

/**
 * Minimal stdout demo: build a tagged series, write it through the in-memory
 * store, read it back, and show the captured change events. No server.
 */
public final class Demo {
    public static void main(String[] args) {
        TsSeriesMetadata meta = new TsSeriesMetadata(7, "cpu")
                .withTag("host", "edge-01")
                .withTag("region", "us-east-1");
        TsSeriesD series = new TsSeriesD();
        series.push(1_780_000_000_000_000_000L, 0.42);
        series.push(1_780_000_001_000_000_000L, 0.55);
        series = series.withMetadata(meta);

        TsMongoAdapter<InMemoryMongoStore> adapter =
                new TsMongoAdapter<>(new InMemoryMongoStore());
        long n = adapter.writeSeries(series);
        System.out.println("wrote " + n + " point documents");

        adapter.ensureIndexes();
        TsSeriesD back = adapter.readSeries(7);
        System.out.println("read back "
                + back.metadata().map(TsSeriesMetadata::name).orElse("?")
                + " (" + back.size() + " points, last "
                + back.last().map(p -> p.value()).orElse(Double.NaN) + ")");

        List<TsChangeEvent> changes = adapter.pollChanges();
        System.out.println("captured " + changes.size() + " change events");
    }
}
