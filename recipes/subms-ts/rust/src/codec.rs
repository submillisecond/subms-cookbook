//! Serialization. [`TsCodec`] is the one substrate every storage format in
//! the arc implements; codec recipes (`subms-ts-cbor`, `subms-ts-gzip`,
//! `subms-gorilla-block`) plug in here and compose by wrapping one another.
//!
//! This crate ships [`TsJsonCodec`] for the scalar `f64` fast path - a
//! zero-dep, human-readable, columnar JSON form that round-trips. The
//! generic-over-`T` JSON surface + binary codecs land in their own recipes.

use crate::TsSeries;

/// How timestamps render in a human-readable codec.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Default)]
pub enum TsTimestampStyle {
    #[default]
    EpochNanos,
    EpochMillis,
    /// `YYYY-MM-DDTHH:MM:SS.fffffffffZ`. Encode-only in 0.6; decoding ISO
    /// timestamps arrives with the `datetime` feature.
    Iso8601,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsCodecError {
    Parse(String),
    UnsupportedTimestampDecode,
}

impl std::fmt::Display for TsCodecError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsCodecError::Parse(m) => write!(f, "parse error: {m}"),
            TsCodecError::UnsupportedTimestampDecode => {
                write!(
                    f,
                    "decoding ISO-8601 timestamps requires the `datetime` feature"
                )
            }
        }
    }
}

impl std::error::Error for TsCodecError {}

/// The codec substrate. One value type `T` per impl; wrappers (gzip, etc.)
/// delegate to an inner codec.
pub trait TsCodec<T> {
    type Error;
    fn encode(&self, series: &TsSeries<T>) -> Vec<u8>;
    fn decode(&self, bytes: &[u8]) -> Result<TsSeries<T>, Self::Error>;
    fn format(&self) -> &str;
}

/// Columnar JSON codec for `TsSeries<f64>`:
/// `{"name":..,"timestamps":[..],"values":[..]}`.
#[derive(Clone, Debug)]
pub struct TsJsonCodec {
    pub style: TsTimestampStyle,
    pub pretty: bool,
}

impl Default for TsJsonCodec {
    fn default() -> Self {
        Self {
            style: TsTimestampStyle::EpochNanos,
            pretty: false,
        }
    }
}

impl TsJsonCodec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_style(mut self, style: TsTimestampStyle) -> Self {
        self.style = style;
        self
    }

    pub fn pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    fn ts_token(&self, ts: i64) -> String {
        match self.style {
            TsTimestampStyle::EpochNanos => ts.to_string(),
            TsTimestampStyle::EpochMillis => (ts / 1_000_000).to_string(),
            TsTimestampStyle::Iso8601 => format!("\"{}\"", iso8601_from_nanos(ts)),
        }
    }
}

impl TsCodec<f64> for TsJsonCodec {
    type Error = TsCodecError;

    fn encode(&self, series: &TsSeries<f64>) -> Vec<u8> {
        let nl = if self.pretty { "\n" } else { "" };
        let sp = if self.pretty { "  " } else { "" };
        let mut out = String::new();
        out.push('{');
        out.push_str(nl);
        if let Some(name) = series.metadata().map(|m| m.name.as_str()) {
            out.push_str(sp);
            out.push_str("\"name\":");
            out.push_str(&json_string(name));
            out.push(',');
            out.push_str(nl);
        }
        out.push_str(sp);
        out.push_str("\"timestamps\":[");
        for (i, p) in series.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&self.ts_token(p.ts));
        }
        out.push_str("],");
        out.push_str(nl);
        out.push_str(sp);
        out.push_str("\"values\":[");
        for (i, p) in series.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&fmt_f64(p.value));
        }
        out.push(']');
        out.push_str(nl);
        out.push('}');
        out.into_bytes()
    }

    fn decode(&self, bytes: &[u8]) -> Result<TsSeries<f64>, Self::Error> {
        let text = std::str::from_utf8(bytes).map_err(|e| TsCodecError::Parse(e.to_string()))?;
        let ts = extract_number_array(text, "timestamps")?;
        let vals = extract_f64_array(text, "values")?;
        if ts.len() != vals.len() {
            return Err(TsCodecError::Parse(format!(
                "timestamps ({}) and values ({}) length mismatch",
                ts.len(),
                vals.len()
            )));
        }
        let scale = match self.style {
            TsTimestampStyle::EpochNanos => 1,
            TsTimestampStyle::EpochMillis => 1_000_000,
            TsTimestampStyle::Iso8601 => return Err(TsCodecError::UnsupportedTimestampDecode),
        };
        let mut s = TsSeries::with_capacity(ts.len());
        for (t, v) in ts.into_iter().zip(vals) {
            s.push(t * scale, v)
                .map_err(|e| TsCodecError::Parse(e.to_string()))?;
        }
        Ok(s)
    }

    fn format(&self) -> &str {
        "json"
    }
}

fn fmt_f64(v: f64) -> String {
    // Values are finite (push rejects NaN/inf). A whole number still prints
    // with a fractional part so the column stays unambiguously float.
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Pull the `[...]` array following `"<key>":` and parse it as i64s. Tolerant
/// of whitespace; built for the shape this codec emits.
fn extract_number_array(text: &str, key: &str) -> Result<Vec<i64>, TsCodecError> {
    let body = array_body(text, key)?;
    let mut out = Vec::new();
    for tok in body.split(',') {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        out.push(
            t.parse::<i64>()
                .map_err(|_| TsCodecError::Parse(format!("bad timestamp token: {t}")))?,
        );
    }
    Ok(out)
}

fn extract_f64_array(text: &str, key: &str) -> Result<Vec<f64>, TsCodecError> {
    let body = array_body(text, key)?;
    let mut out = Vec::new();
    for tok in body.split(',') {
        let t = tok.trim();
        if t.is_empty() {
            continue;
        }
        out.push(
            t.parse::<f64>()
                .map_err(|_| TsCodecError::Parse(format!("bad value token: {t}")))?,
        );
    }
    Ok(out)
}

fn array_body<'a>(text: &'a str, key: &str) -> Result<&'a str, TsCodecError> {
    let needle = format!("\"{key}\"");
    let kpos = text
        .find(&needle)
        .ok_or_else(|| TsCodecError::Parse(format!("missing key {key}")))?;
    let after = &text[kpos + needle.len()..];
    let open = after
        .find('[')
        .ok_or_else(|| TsCodecError::Parse(format!("no array after {key}")))?;
    let rest = &after[open + 1..];
    let close = rest
        .find(']')
        .ok_or_else(|| TsCodecError::Parse(format!("unterminated array for {key}")))?;
    Ok(&rest[..close])
}

/// Civil date from epoch-nanoseconds, formatted ISO-8601 UTC. Hand-rolled
/// (Hinnant's algorithm) to keep the codec zero-dep.
fn iso8601_from_nanos(ns: i64) -> String {
    let secs = ns.div_euclid(1_000_000_000);
    let sub_ns = ns.rem_euclid(1_000_000_000);
    let days = secs.div_euclid(86_400);
    let secs_of_day = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let hh = secs_of_day / 3_600;
    let mm = (secs_of_day % 3_600) / 60;
    let ss = secs_of_day % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{sub_ns:09}Z")
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}
