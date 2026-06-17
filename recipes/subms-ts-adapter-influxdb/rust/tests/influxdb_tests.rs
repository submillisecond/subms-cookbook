use subms_ts::{TsCollection, TsSeries, TsSeriesMetadata};
use subms_ts_influxdb::{
    CaptureTransport, TsHttpResponse, TsInfluxAdapter, TsInfluxError, decode_response,
    encode_collection, encode_line, encode_series, format_rfc3339_nanos, parse_rfc3339_nanos,
};

fn tagged(name: &str, id: u64, tags: &[(&str, &str)]) -> TsSeriesMetadata {
    let mut m = TsSeriesMetadata::new(id, name);
    for (k, v) in tags {
        m = m.with_tag(*k, *v);
    }
    m
}

#[test]
fn rfc3339_roundtrip_whole_second() {
    let ts = parse_rfc3339_nanos("2026-05-31T14:00:00Z").unwrap();
    assert_eq!(format_rfc3339_nanos(ts), "2026-05-31T14:00:00Z");
}

#[test]
fn rfc3339_roundtrip_with_fraction() {
    let ts = parse_rfc3339_nanos("2026-05-31T14:00:00.123456789Z").unwrap();
    assert_eq!(format_rfc3339_nanos(ts), "2026-05-31T14:00:00.123456789Z");
}

#[test]
fn rfc3339_parses_short_fraction() {
    let a = parse_rfc3339_nanos("2026-01-01T00:00:00.5Z").unwrap();
    assert_eq!(
        a,
        parse_rfc3339_nanos("2026-01-01T00:00:00Z").unwrap() + 500_000_000
    );
}

#[test]
fn rfc3339_rejects_garbage() {
    assert!(parse_rfc3339_nanos("not-a-time").is_none());
    assert!(parse_rfc3339_nanos("2026-13-01T00:00:00Z").is_none());
    assert!(parse_rfc3339_nanos("2026-05-31T14:00:00").is_none()); // no Z
}

#[test]
fn encode_line_escapes_specials() {
    let mut out = String::new();
    encode_line(
        "cpu load",
        &[("data center", "us east,1")],
        1.5,
        42,
        &mut out,
    );
    assert_eq!(out, "cpu\\ load,data\\ center=us\\ east\\,1 v=1.5 42");
}

#[test]
fn encode_line_integer_value_keeps_decimal() {
    let mut out = String::new();
    encode_line("m", &[], 100.0, 7, &mut out);
    assert_eq!(out, "m v=100.0 7");
}

#[test]
fn encode_series_uses_metadata() {
    let mut s = TsSeries::<f64>::new();
    s.push(10, 0.5).unwrap();
    s.push(20, 0.75).unwrap();
    let s = s.with_metadata(tagged("cpu", 1, &[("host", "a"), ("region", "eu")]));
    let body = encode_series(&s, "");
    assert_eq!(
        body,
        "cpu,host=a,region=eu v=0.5 10\ncpu,host=a,region=eu v=0.75 20"
    );
}

#[test]
fn encode_series_measurement_override() {
    let mut s = TsSeries::<f64>::new();
    s.push(10, 1.0).unwrap();
    let s = s.with_metadata(tagged("ignored", 1, &[]));
    let body = encode_series(&s, "explicit");
    assert!(body.starts_with("explicit v=1.0 10"));
}

#[test]
fn encode_collection_one_block_per_series() {
    let mut coll = TsCollection::<f64>::new();
    let a = coll.register(tagged("cpu", 1, &[("host", "a")])).unwrap();
    let b = coll.register(tagged("mem", 2, &[("host", "b")])).unwrap();
    coll.push(a, 1, 0.1).unwrap();
    coll.push(b, 1, 0.2).unwrap();
    let body = encode_collection(&coll);
    assert!(body.contains("cpu,host=a v=0.1 1"));
    assert!(body.contains("mem,host=b v=0.2 1"));
    assert_eq!(body.lines().count(), 2);
}

const CSV: &str = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n\
,result,table,_time,_value,_field,_measurement,host\n\
,_result,0,2026-05-31T14:00:00Z,0.42,v,cpu,edge-01\n\
,_result,0,2026-05-31T14:00:01Z,0.55,v,cpu,edge-01\n";

#[test]
fn decode_basic_single_series() {
    let coll = decode_response(CSV).unwrap();
    assert_eq!(coll.len(), 1);
    let s = coll.by_name("cpu,host=edge-01").unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s.first().unwrap().value, 0.42);
    assert_eq!(s.last().unwrap().value, 0.55);
}

#[test]
fn decode_reconstructs_tags_and_multiple_series() {
    let csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n\
,result,table,_time,_value,_field,_measurement,host\n\
,_result,0,2026-05-31T14:00:00Z,1.0,v,cpu,a\n\
,_result,1,2026-05-31T14:00:00Z,2.0,v,cpu,b\n";
    let coll = decode_response(csv).unwrap();
    assert_eq!(coll.len(), 2);
    let a = coll.by_tag("host", "a").next().unwrap();
    assert_eq!(a.first().unwrap().value, 1.0);
}

#[test]
fn decode_handles_quoted_fields() {
    let csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n\
,result,table,_time,_value,_field,_measurement,host\n\
,_result,0,2026-05-31T14:00:00Z,3.5,v,\"cpu,total\",\"east, dc\"\n";
    let coll = decode_response(csv).unwrap();
    assert_eq!(coll.len(), 1);
    assert!(coll.by_tag("host", "east, dc").next().is_some());
}

#[test]
fn decode_orders_points_by_time() {
    let csv = "#datatype,string,long,dateTime:RFC3339,double,string,string,string\n\
,result,table,_time,_value,_field,_measurement,host\n\
,_result,0,2026-05-31T14:00:02Z,2.0,v,cpu,a\n\
,_result,0,2026-05-31T14:00:00Z,0.0,v,cpu,a\n\
,_result,0,2026-05-31T14:00:01Z,1.0,v,cpu,a\n";
    let coll = decode_response(csv).unwrap();
    let s = coll.by_name("cpu,host=a").unwrap();
    let vals: Vec<f64> = s.iter().map(|p| p.value).collect();
    assert_eq!(vals, vec![0.0, 1.0, 2.0]);
}

#[test]
fn decode_rejects_response_without_time_value() {
    let err = decode_response("col1,col2\n1,2\n").unwrap_err();
    assert!(matches!(err, TsInfluxError::Csv { .. }));
}

#[test]
fn adapter_write_series_builds_request() {
    let cap = CaptureTransport::ok("");
    let mut s = TsSeries::<f64>::new();
    s.push(10, 0.5).unwrap();
    let s = s.with_metadata(tagged("cpu", 1, &[("host", "a")]));
    let adapter = TsInfluxAdapter::with_transport(cap, "tok", "myorg", "mybucket");
    let n = adapter.write_series(&s, "").unwrap();
    assert_eq!(n, 1);
}

#[test]
fn adapter_write_request_has_path_headers_body() {
    let cap = CaptureTransport::ok("");
    let mut s = TsSeries::<f64>::new();
    s.push(10, 0.5).unwrap();
    let s = s.with_metadata(tagged("cpu", 1, &[("host", "a")]));
    let adapter = TsInfluxAdapter::with_transport(cap, "secrettok", "my org", "b");
    adapter.write_series(&s, "").unwrap();

    let sent = adapter.transport().sent.borrow();
    let req = sent.last().unwrap();
    assert_eq!(req.method, "POST");
    assert!(req.path.starts_with("/api/v2/write?"));
    assert!(req.path.contains("precision=ns"));
    assert!(req.path.contains("org=my%20org")); // percent-encoded
    assert!(req.path.contains("bucket=b"));
    assert!(
        req.headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v == "Token secrettok")
    );
    assert_eq!(req.body, "cpu,host=a v=0.5 10");
}

#[test]
fn adapter_write_empty_series_is_noop() {
    let cap = CaptureTransport::ok("");
    let s = TsSeries::<f64>::new().with_metadata(tagged("cpu", 1, &[]));
    let adapter = TsInfluxAdapter::with_transport(cap, "t", "o", "b");
    assert_eq!(adapter.write_series(&s, "").unwrap(), 0);
}

#[test]
fn adapter_query_flux_decodes() {
    let cap = CaptureTransport::ok(CSV);
    let adapter = TsInfluxAdapter::with_transport(cap, "t", "o", "b");
    let coll = adapter.query_flux("from(bucket:\"b\")").unwrap();
    assert_eq!(coll.len(), 1);
    assert_eq!(coll.by_name("cpu,host=edge-01").unwrap().len(), 2);
}

#[test]
fn adapter_http_error_surfaces() {
    let cap = CaptureTransport::new(vec![TsHttpResponse {
        status: 500,
        body: "boom".into(),
    }]);
    let mut s = TsSeries::<f64>::new();
    s.push(10, 0.5).unwrap();
    let s = s.with_metadata(tagged("cpu", 1, &[]));
    let adapter = TsInfluxAdapter::with_transport(cap, "t", "o", "b");
    let err = adapter.write_series(&s, "").unwrap_err();
    assert!(matches!(err, TsInfluxError::Http { status: 500, .. }));
}

#[test]
fn connect_rejects_https() {
    assert!(matches!(
        TsInfluxAdapter::connect("https://localhost:8086", "t", "o", "b"),
        Err(TsInfluxError::Config { .. })
    ));
}

#[test]
fn connect_parses_host_and_default_port() {
    assert!(TsInfluxAdapter::connect("http://localhost", "t", "o", "b").is_ok());
    assert!(TsInfluxAdapter::connect("http://localhost:9999", "t", "o", "b").is_ok());
    assert!(TsInfluxAdapter::connect("http://", "t", "o", "b").is_err());
}

#[test]
fn adapter_query_request_uses_flux_content_type() {
    let cap = CaptureTransport::ok(CSV);
    let adapter = TsInfluxAdapter::with_transport(cap, "tok", "o", "b");
    adapter.query_flux("from(bucket:\"b\")").unwrap();
    let sent = adapter.transport().sent.borrow();
    let req = sent.last().unwrap();
    assert!(req.path.starts_with("/api/v2/query?"));
    assert!(
        req.headers
            .iter()
            .any(|(k, v)| k == "Content-Type" && v == "application/vnd.flux")
    );
    assert_eq!(req.body, "from(bucket:\"b\")");
}
