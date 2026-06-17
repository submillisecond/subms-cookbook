//! `subms-ts-adapter-yaml` - a human-readable YAML codec for `TsSeries<f64>`. Implements
//! the [`TsCodec`](subms_ts::TsCodec) substrate from `subms-ts` with a clean,
//! multi-line columnar layout: two block sequences, `timestamps:` and
//! `values:`, under a `subms_ts_series` document root.
//!
//! This is a `category: adapter` recipe. Encoding is hand-written so the output
//! stays a tidy, diff-friendly columnar block; decoding goes through the
//! [`saphyr`] YAML 1.2 parser, because parsing arbitrary YAML back into a series
//! is where a real parser earns its place - block versus flow sequences,
//! quoting, comments, indentation, and the YAML core-schema scalar rules are
//! thousands of lines of corner cases we do not want to reimplement.
//!
//! Timestamps render per a [`TsTimestampStyle`], mirroring `subms-ts`'s JSON
//! codec: `EpochNanos` and `EpochMillis` are integer columns that round-trip;
//! `Iso8601` is an encode-only rendering (decoding ISO timestamps arrives with
//! the `datetime` feature, same carve-out the JSON codec makes). Like the JSON
//! and CBOR codecs, the wire carries the data columns only - series metadata is
//! not part of the document.
//!
//! ```
//! use subms_ts::{TsCodec, TsSeries};
//! use subms_ts_yaml::TsYamlCodec;
//!
//! let mut s = TsSeries::<f64>::new();
//! s.push(1, 1.5).unwrap();
//! s.push(2, 2.5).unwrap();
//! let codec = TsYamlCodec::new();
//! let bytes = codec.encode(&s);
//! let back = codec.decode(&bytes).unwrap();
//! assert_eq!(back.len(), 2);
//! ```

use saphyr::{LoadableYamlNode, Yaml};
use subms_ts::{TsCodec, TsSeries, TsTimestampStyle};

#[cfg(feature = "harness")]
pub mod recipe;

const ROOT_KEY: &str = "subms_ts_series";
const TS_KEY: &str = "timestamps";
const VAL_KEY: &str = "values";

/// Human-readable YAML codec for the scalar `f64` series. The `style` controls
/// how timestamps render; values are always YAML float scalars.
#[derive(Clone, Debug)]
pub struct TsYamlCodec {
    pub style: TsTimestampStyle,
}

impl Default for TsYamlCodec {
    fn default() -> Self {
        Self {
            style: TsTimestampStyle::EpochNanos,
        }
    }
}

impl TsYamlCodec {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_style(mut self, style: TsTimestampStyle) -> Self {
        self.style = style;
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

/// Failure decoding a YAML buffer that is not a well-formed series document.
/// Mirrors `subms-ts`'s `TsCodecError`: [`TsYamlError::Parse`] for malformed
/// YAML or a shape the grammar does not expect, and
/// [`TsYamlError::UnsupportedTimestampDecode`] for the `Iso8601` style, which is
/// encode-only until the `datetime` feature lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsYamlError {
    Parse(String),
    UnsupportedTimestampDecode,
}

impl std::fmt::Display for TsYamlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsYamlError::Parse(m) => write!(f, "yaml parse error: {m}"),
            TsYamlError::UnsupportedTimestampDecode => {
                write!(
                    f,
                    "decoding ISO-8601 timestamps requires the `datetime` feature"
                )
            }
        }
    }
}

impl std::error::Error for TsYamlError {}

impl TsCodec<f64> for TsYamlCodec {
    type Error = TsYamlError;

    fn encode(&self, series: &TsSeries<f64>) -> Vec<u8> {
        // Emitted by hand rather than through the library's dumper: the columnar
        // block layout is fully under our control and we want the tidy two-list
        // shape, not whatever flow/block heuristic a general emitter picks.
        let n = series.len();
        let mut out = String::with_capacity(64 + n * 24);
        out.push_str(ROOT_KEY);
        out.push_str(":\n");
        out.push_str("  ");
        out.push_str(TS_KEY);
        out.push(':');
        if n == 0 {
            out.push_str(" []\n");
        } else {
            out.push('\n');
            for p in series.iter() {
                out.push_str("  - ");
                out.push_str(&self.ts_token(p.ts));
                out.push('\n');
            }
        }
        out.push_str("  ");
        out.push_str(VAL_KEY);
        out.push(':');
        if n == 0 {
            out.push_str(" []\n");
        } else {
            out.push('\n');
            for p in series.iter() {
                out.push_str("  - ");
                out.push_str(&fmt_f64(p.value));
                out.push('\n');
            }
        }
        out.into_bytes()
    }

    fn decode(&self, bytes: &[u8]) -> Result<TsSeries<f64>, Self::Error> {
        let scale = match self.style {
            TsTimestampStyle::EpochNanos => 1,
            TsTimestampStyle::EpochMillis => 1_000_000,
            TsTimestampStyle::Iso8601 => return Err(TsYamlError::UnsupportedTimestampDecode),
        };

        let text = std::str::from_utf8(bytes).map_err(|e| TsYamlError::Parse(e.to_string()))?;
        let docs = Yaml::load_from_str(text).map_err(|e| TsYamlError::Parse(e.to_string()))?;
        let doc = docs
            .first()
            .ok_or_else(|| TsYamlError::Parse("empty document".into()))?;
        let root = doc
            .as_mapping_get(ROOT_KEY)
            .ok_or_else(|| TsYamlError::Parse(format!("missing `{ROOT_KEY}` mapping")))?;

        let ts_seq = root
            .as_mapping_get(TS_KEY)
            .ok_or_else(|| TsYamlError::Parse(format!("missing `{TS_KEY}` sequence")))?;
        let val_seq = root
            .as_mapping_get(VAL_KEY)
            .ok_or_else(|| TsYamlError::Parse(format!("missing `{VAL_KEY}` sequence")))?;

        let ts = read_int_seq(ts_seq, TS_KEY)?;
        let vals = read_f64_seq(val_seq, VAL_KEY)?;
        if ts.len() != vals.len() {
            return Err(TsYamlError::Parse(format!(
                "timestamps ({}) and values ({}) length mismatch",
                ts.len(),
                vals.len()
            )));
        }

        let mut s = TsSeries::with_capacity(ts.len());
        for (t, v) in ts.into_iter().zip(vals) {
            s.push(t * scale, v)
                .map_err(|e| TsYamlError::Parse(e.to_string()))?;
        }
        Ok(s)
    }

    fn format(&self) -> &str {
        "yaml"
    }
}

fn read_int_seq(node: &Yaml, key: &str) -> Result<Vec<i64>, TsYamlError> {
    let seq = node
        .as_vec()
        .ok_or_else(|| TsYamlError::Parse(format!("`{key}` is not a sequence")))?;
    let mut out = Vec::with_capacity(seq.len());
    for item in seq {
        let v = item
            .as_integer()
            .ok_or_else(|| TsYamlError::Parse(format!("`{key}` item is not an integer")))?;
        out.push(v);
    }
    Ok(out)
}

fn read_f64_seq(node: &Yaml, key: &str) -> Result<Vec<f64>, TsYamlError> {
    let seq = node
        .as_vec()
        .ok_or_else(|| TsYamlError::Parse(format!("`{key}` is not a sequence")))?;
    let mut out = Vec::with_capacity(seq.len());
    for item in seq {
        // A whole-number value may parse as a YAML integer (`3`) rather than a
        // float (`3.0`); accept either so a hand-edited document still decodes.
        let v = item
            .as_floating_point()
            .or_else(|| item.as_integer().map(|i| i as f64))
            .ok_or_else(|| TsYamlError::Parse(format!("`{key}` item is not a number")))?;
        out.push(v);
    }
    Ok(out)
}

/// Shortest round-trippable `f64` rendering. A whole number still prints with a
/// fractional part so the column stays unambiguously float (matching the JSON
/// codec). The series rejects NaN/inf at push, so the value is finite here.
fn fmt_f64(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Civil date from epoch-nanoseconds, formatted ISO-8601 UTC. Hand-rolled
/// (Hinnant's algorithm), the same rendering the `subms-ts` JSON codec uses.
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
