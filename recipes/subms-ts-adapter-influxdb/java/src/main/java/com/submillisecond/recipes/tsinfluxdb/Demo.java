package com.submillisecond.recipes.tsinfluxdb;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsSeriesD;
import com.submillisecond.recipes.ts.TsSeriesMetadata;

/**
 * Minimal stdout demo: build a tagged series, encode it to line protocol, then
 * decode a Flux-shaped CSV back into a collection. No network.
 */
public final class Demo {
    public static void main(String[] args) {
        TsSeriesMetadata meta = new TsSeriesMetadata(1, "cpu")
                .withTag("host", "edge-01")
                .withTag("region", "us-east-1");
        TsSeriesD series = new TsSeriesD();
        series.push(1_780_000_000_000_000_000L, 0.42);
        series.push(1_780_000_001_000_000_000L, 0.55);
        series = series.withMetadata(meta);

        System.out.println("line protocol:");
        System.out.println(LineProtocol.encodeSeries(series, ""));

        String csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n"
                + ",result,table,_time,_value,_field,_measurement,host\n"
                + ",_result,0,2026-05-31T14:00:00Z,0.42,v,cpu,edge-01\n"
                + ",_result,0,2026-05-31T14:00:01Z,0.55,v,cpu,edge-01\n";
        TsCollection<Double> coll = FluxCsv.decodeResponse(csv);
        System.out.println("\ndecoded " + coll.size() + " series from the Flux response");
        coll.series().stream().findFirst().ifPresent(s -> System.out.println(
                s.metadata().map(TsSeriesMetadata::name).orElse("?")
                + " has " + s.size() + " points"));
    }
}
