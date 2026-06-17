//! Decode a Flux annotated-CSV query response into a `TsCollection<f64>`.
//!
//! Flux emits RFC4180 CSV with leading annotation rows (`#datatype`, `#group`,
//! `#default`). We ignore the annotations and key off the header names:
//! `_time` + `_value` carry the point, `_measurement` plus any non-reserved
//! column reconstruct the series identity. One series per
//! (measurement, tag-set); points land in time order.

use crate::error::TsInfluxError;
use crate::time::parse_rfc3339_nanos;
use subms_ts::{TsCollection, TsSeriesMetadata};

const RESERVED: [&str; 8] = [
    "", "result", "table", "_start", "_stop", "_time", "_value", "_field",
];

/// Influx series key: `measurement` when untagged, else
/// `measurement,k=v,...` with tags sorted by key. Unique per (measurement,
/// tag-set), which is the identity the collection needs.
fn series_key(measurement: &str, tags: &[(String, String)]) -> String {
    if tags.is_empty() {
        return measurement.to_string();
    }
    let mut sorted: Vec<&(String, String)> = tags.iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(&b.0));
    let mut key = measurement.to_string();
    for (k, v) in sorted {
        key.push(',');
        key.push_str(k);
        key.push('=');
        key.push_str(v);
    }
    key
}

/// Tokenise RFC4180 CSV into records of fields, honouring quoted fields with
/// embedded commas, doubled quotes, and newlines.
fn parse_records(input: &str) -> Vec<Vec<String>> {
    let mut records = Vec::new();
    let mut field = String::new();
    let mut record = Vec::new();
    let mut in_quotes = false;
    let mut chars = input.chars().peekable();
    let mut saw_field = false;

    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
        } else {
            match c {
                '"' => {
                    in_quotes = true;
                    saw_field = true;
                }
                ',' => {
                    record.push(std::mem::take(&mut field));
                    saw_field = true;
                }
                '\r' => {}
                '\n' => {
                    if saw_field || !field.is_empty() || !record.is_empty() {
                        record.push(std::mem::take(&mut field));
                        records.push(std::mem::take(&mut record));
                    }
                    saw_field = false;
                }
                _ => {
                    field.push(c);
                    saw_field = true;
                }
            }
        }
    }
    if saw_field || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }
    records
}

/// Decode an annotated-CSV Flux response.
pub fn decode_response(body: &str) -> Result<TsCollection<f64>, TsInfluxError> {
    let records = parse_records(body);
    let header = records
        .iter()
        .find(|r| r.iter().any(|f| f == "_time") && r.iter().any(|f| f == "_value"))
        .ok_or_else(|| TsInfluxError::csv("no header row with _time and _value"))?;

    let col = |name: &str| header.iter().position(|f| f == name);
    let time_i = col("_time").ok_or_else(|| TsInfluxError::csv("missing _time"))?;
    let value_i = col("_value").ok_or_else(|| TsInfluxError::csv("missing _value"))?;
    let meas_i = col("_measurement");
    let tag_cols: Vec<(usize, &str)> = header
        .iter()
        .enumerate()
        .filter(|(_, n)| !RESERVED.contains(&n.as_str()) && *n != "_measurement")
        .map(|(i, n)| (i, n.as_str()))
        .collect();

    let mut coll = TsCollection::<f64>::new();
    let mut keyed: std::collections::HashMap<String, (u64, Vec<(i64, f64)>)> =
        std::collections::HashMap::new();
    let mut next_id: u64 = 0;

    for row in &records {
        if row.len() < header.len() || std::ptr::eq(row, header) {
            continue;
        }
        if row.iter().any(|f| f.starts_with('#')) {
            continue;
        }
        let raw_time = &row[time_i];
        if raw_time == "_time" || raw_time.is_empty() {
            continue;
        }
        let ts =
            parse_rfc3339_nanos(raw_time).ok_or_else(|| TsInfluxError::csv("unparseable _time"))?;
        let value: f64 = row[value_i]
            .parse()
            .map_err(|_| TsInfluxError::csv("unparseable _value"))?;
        let measurement = meas_i.map(|i| row[i].clone()).unwrap_or_default();

        let mut key = measurement.clone();
        let mut tags: Vec<(String, String)> = Vec::new();
        for (i, name) in &tag_cols {
            let v = &row[*i];
            if v.is_empty() {
                continue;
            }
            key.push('\u{1}');
            key.push_str(name);
            key.push('=');
            key.push_str(v);
            tags.push(((*name).to_string(), v.clone()));
        }

        let entry = keyed.entry(key).or_insert_with(|| {
            let id = next_id;
            next_id += 1;
            // The collection keys series by a unique name, but Influx allows
            // many series per measurement (one per tag set). Name each by its
            // Influx series key (`measurement,k=v,...`, tags sorted) so the
            // identities stay distinct; the bare measurement + tags remain
            // queryable via by_tag.
            let mut meta = TsSeriesMetadata::new(id, series_key(&measurement, &tags));
            for (k, v) in &tags {
                meta = meta.with_tag(k.clone(), v.clone());
            }
            let id = coll.register(meta).expect("unique series id");
            (id, Vec::new())
        });
        entry.1.push((ts, value));
    }

    for (id, mut points) in keyed.into_values() {
        points.sort_by_key(|(ts, _)| *ts);
        for (ts, v) in points {
            coll.push(id, ts, v)
                .map_err(|e| TsInfluxError::csv(format!("push: {e:?}")))?;
        }
    }
    Ok(coll)
}
