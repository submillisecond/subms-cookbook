package com.submillisecond.recipes.tsarrow;

import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import org.apache.arrow.memory.RootAllocator;

/**
 * Minimal stdout demo: build a tagged series, convert it to an Arrow root,
 * round-trip it through an IPC stream, and read it back. Run with
 * {@code --add-opens=java.base/java.nio=ALL-UNNAMED}.
 */
public final class Demo {
    public static void main(String[] args) {
        TsSeriesMetadata meta = new TsSeriesMetadata(1, "cpu").withTag("host", "edge-01");
        TsSeriesD series = new TsSeriesD();
        series.push(1_780_000_000_000_000_000L, 0.42);
        series.push(1_780_000_001_000_000_000L, 0.55);
        series = series.withMetadata(meta);

        try (RootAllocator alloc = new RootAllocator()) {
            byte[] ipc = ArrowConvert.seriesToIpc(series, alloc);
            System.out.println("ipc stream: " + ipc.length + " bytes");
            TsSeriesD back = ArrowConvert.ipcToSeries(ipc, alloc);
            System.out.println("read back "
                    + back.metadata().map(TsSeriesMetadata::name).orElse("?")
                    + " (" + back.size() + " points, last "
                    + back.last().map(p -> p.value()).orElse(Double.NaN) + ")");
        }
    }
}
