package com.submillisecond.recipes.tsparquet;

import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/** Minimal stdout demo: persist a tagged series to Parquet bytes, read it back. */
public final class Demo {
    public static void main(String[] args) {
        TsSeriesMetadata meta = new TsSeriesMetadata(1, "cpu").withTag("host", "edge-01");
        TsSeriesD series = new TsSeriesD();
        series.push(1_780_000_000_000_000_000L, 0.42);
        series.push(1_780_000_001_000_000_000L, 0.55);
        series = series.withMetadata(meta);

        byte[] bytes = ParquetConvert.seriesToParquet(series);
        System.out.println("parquet file: " + bytes.length + " bytes");

        TsSeriesD back = ParquetConvert.parquetToSeries(bytes);
        System.out.println("read back "
                + back.metadata().map(TsSeriesMetadata::name).orElse("?")
                + " (" + back.size() + " points, last "
                + back.last().map(p -> p.value()).orElse(Double.NaN) + ")");
    }
}
