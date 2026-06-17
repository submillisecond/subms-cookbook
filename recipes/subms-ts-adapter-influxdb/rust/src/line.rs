//! InfluxDB line protocol encoder, zero-dep.
//!
//! Shape per point:
//! `measurement[,tagkey=tagval...] field=value timestamp`
//! Tags are emitted in key order (a `TsTags` is a `BTreeMap`, so already
//! sorted, which is what Influx wants for ingest locality). The field key is
//! fixed to `v` - one numeric field per point matches the `TsSeries<f64>`
//! shape. Timestamps are nanoseconds (the write call sets `precision=ns`).

use subms_ts::{TsCollection, TsSeries};

/// Escape a measurement name: commas and spaces are special; `=` is not.
pub fn escape_measurement(s: &str) -> String {
    escape(s, &[',', ' '])
}

/// Escape a tag key, tag value, or field key: commas, equals, spaces.
pub fn escape_tag(s: &str) -> String {
    escape(s, &[',', '=', ' '])
}

fn escape(s: &str, specials: &[char]) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if specials.contains(&c) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Render an f64 field value. Influx treats a bare number as a float; we keep
/// full round-trip precision and never emit a non-finite token (the caller is
/// expected to have filtered NaN/inf, which `TsSeries` rejects on ingest).
fn fmt_value(v: f64) -> String {
    if v == v.trunc() && v.is_finite() && v.abs() < 1e16 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Encode one point line (no trailing newline) into `out`.
pub fn encode_line(
    measurement: &str,
    tags: &[(&str, &str)],
    value: f64,
    ts: i64,
    out: &mut String,
) {
    out.push_str(&escape_measurement(measurement));
    for (k, val) in tags {
        out.push(',');
        out.push_str(&escape_tag(k));
        out.push('=');
        out.push_str(&escape_tag(val));
    }
    out.push_str(" v=");
    out.push_str(&fmt_value(value));
    out.push(' ');
    out.push_str(&ts.to_string());
}

/// Encode a whole series as a line-protocol batch. The measurement defaults to
/// the series metadata name when `measurement` is empty; tags come from the
/// series metadata.
pub fn encode_series(series: &TsSeries<f64>, measurement: &str) -> String {
    let meta = series.metadata();
    let name = if !measurement.is_empty() {
        measurement.to_string()
    } else {
        meta.map(|m| m.name.clone()).unwrap_or_default()
    };
    let tags: Vec<(&str, &str)> = meta
        .map(|m| {
            m.tags
                .iter()
                .map(|(k, v)| (k.as_str(), v.as_str()))
                .collect()
        })
        .unwrap_or_default();

    let mut out = String::new();
    for p in series.iter() {
        if !out.is_empty() {
            out.push('\n');
        }
        encode_line(&name, &tags, p.value, p.ts, &mut out);
    }
    out
}

/// Encode every series in a collection, one measurement per series (named by
/// each series' metadata name).
pub fn encode_collection(coll: &TsCollection<f64>) -> String {
    let mut out = String::new();
    for s in coll.series() {
        let body = encode_series(s, "");
        if body.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(&body);
    }
    out
}
