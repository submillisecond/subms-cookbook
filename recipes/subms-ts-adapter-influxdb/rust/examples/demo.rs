//! Minimal stdout demo: build a tagged series, encode it to line protocol,
//! then decode a Flux-shaped CSV back into a collection. No network.

use subms_ts::{TsSeries, TsSeriesMetadata};
use subms_ts_influxdb::{decode_response, encode_series};

fn main() {
    let meta = TsSeriesMetadata::new(1, "cpu")
        .with_tag("host", "edge-01")
        .with_tag("region", "us-east-1");
    let mut series = TsSeries::<f64>::new();
    series.push(1_780_000_000_000_000_000, 0.42).unwrap();
    series.push(1_780_000_001_000_000_000, 0.55).unwrap();
    let series = series.with_metadata(meta);

    println!("line protocol:");
    println!("{}", encode_series(&series, ""));

    let csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n\
               ,result,table,_time,_value,_field,_measurement,host\n\
               ,_result,0,2026-05-31T14:00:00Z,0.42,v,cpu,edge-01\n\
               ,_result,0,2026-05-31T14:00:01Z,0.55,v,cpu,edge-01\n";
    let coll = decode_response(csv).unwrap();
    println!("\ndecoded {} series from the Flux response", coll.len());
    if let Some(s) = coll.series().next() {
        println!(
            "{} has {} points, last value {:?}",
            s.metadata().map(|m| m.name.as_str()).unwrap_or("?"),
            s.len(),
            s.last().map(|p| p.value)
        );
    }
}
