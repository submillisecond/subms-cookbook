//! `subms-ts-csv` - a zero-dependency, hand-rolled CSV / NDJSON reader and
//! writer for the typed [`TsDataFrame`]. Each source column becomes a typed
//! [`TsColumn`] (`TsSeries<T>`) over a row-index or designated timestamp axis,
//! with the element type inferred per column by the narrowest-fit rule:
//! `I64` if every non-empty cell parses as an `i64`, else `F64` if every cell
//! parses as an `f64`, else `Bool` if every cell is `true`/`false`, else
//! `Str`. An empty cell is a gap - the row simply contributes no point to that
//! column, mirroring the frame's gap model. No null is ever pushed.
//!
//! The parser is RFC-4180-ish: comma separator (configurable), double-quote
//! quoting with `""` as the embedded-quote escape, and CRLF or LF line
//! endings. NDJSON ingest reads one flat JSON object per line and takes the
//! union of keys as the column set; a key absent on a line is a gap.
//!
//! ```
//! use subms_ts_csv::{read_csv, TsCsvOptions};
//! use subms_ts::TsDataType;
//!
//! let text = "t,price,ok\n1,10.5,true\n2,11.0,false\n";
//! let df = read_csv(text, &TsCsvOptions::default().ts_column("t")).unwrap();
//! assert_eq!(df.column("price").unwrap().data_type(), TsDataType::F64);
//! assert_eq!(df.column("ok").unwrap().data_type(), TsDataType::Bool);
//! // the ts column drives the axis and is not re-emitted as a value column.
//! assert!(df.column("t").is_none());
//! ```

use subms_ts::{TsColumn, TsDataFrame, TsSeries};

mod csv;
mod infer;
mod ndjson;

#[cfg(feature = "harness")]
pub mod recipe;

pub use csv::{read_csv, write_csv};
pub use infer::TsInferredType;
pub use ndjson::read_ndjson;

/// Reader / writer options. `delimiter` is the CSV field separator (default
/// `,`); `has_header` toggles whether the first row names the columns (when
/// `false`, columns are synthesised `col0..colN`); `ts_column`, when set, names
/// the column whose `i64` cells become the frame's timestamp axis (that column
/// is consumed, not re-emitted as a value column).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsCsvOptions {
    pub has_header: bool,
    pub ts_column: Option<String>,
    pub delimiter: char,
}

impl Default for TsCsvOptions {
    fn default() -> Self {
        Self {
            has_header: true,
            ts_column: None,
            delimiter: ',',
        }
    }
}

impl TsCsvOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn has_header(mut self, yes: bool) -> Self {
        self.has_header = yes;
        self
    }

    pub fn ts_column(mut self, name: impl Into<String>) -> Self {
        self.ts_column = Some(name.into());
        self
    }

    pub fn delimiter(mut self, delim: char) -> Self {
        self.delimiter = delim;
        self
    }
}

/// Errors from the reader. The writer is total - any frame serialises - so it
/// returns a `String`, not a `Result`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsCsvError {
    /// A data row had a field count that did not match the header / first row.
    RaggedRow {
        row: usize,
        expected: usize,
        got: usize,
    },
    /// A quoted field was opened but the input ended before its closing quote,
    /// or a character followed a closing quote inside a field.
    BadQuoting { row: usize },
    /// `ts_column` named a column whose cell did not parse as an `i64`.
    BadTimestamp { row: usize, value: String },
    /// `ts_column` named a column that the header does not contain.
    UnknownTsColumn { name: String },
    /// An NDJSON line was not a flat JSON object, or was malformed.
    BadJson { line: usize, hint: &'static str },
}

impl std::fmt::Display for TsCsvError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsCsvError::RaggedRow {
                row,
                expected,
                got,
            } => write!(f, "ragged row {row}: expected {expected} fields, got {got}"),
            TsCsvError::BadQuoting { row } => write!(f, "bad quoting in row {row}"),
            TsCsvError::BadTimestamp { row, value } => {
                write!(f, "ts_column value in row {row} is not an i64: {value:?}")
            }
            TsCsvError::UnknownTsColumn { name } => write!(f, "unknown ts_column: {name}"),
            TsCsvError::BadJson { line, hint } => write!(f, "bad json on line {line}: {hint}"),
        }
    }
}

impl std::error::Error for TsCsvError {}

/// A column's cells in parallel with the row timestamps that survived (an
/// empty cell drops its `(ts, raw)` pair). Built once per column during a read,
/// then handed to [`RawColumn::build`]. `forced_str` pins the column to `Str`
/// regardless of cell shape - the NDJSON reader sets it when a value arrived
/// quoted, so a quoted `"1"` stays text rather than re-inferring to `I64`.
pub(crate) struct RawColumn {
    pub cells: Vec<(i64, String)>,
    pub forced_str: bool,
}

impl RawColumn {
    pub fn new() -> Self {
        Self {
            cells: Vec::new(),
            forced_str: false,
        }
    }

    /// Infer the narrowest element type that fits every cell, then materialise
    /// the typed column. An empty cell never reaches here (gaps are dropped at
    /// ingest), so an all-empty column is `Str` with zero points.
    pub fn build(self) -> TsColumn {
        let kind = if self.forced_str {
            TsInferredType::Str
        } else {
            infer::infer(self.cells.iter().map(|(_, c)| c.as_str()))
        };
        match kind {
            TsInferredType::I64 => {
                let mut s = TsSeries::<i64>::with_capacity(self.cells.len());
                for (ts, c) in &self.cells {
                    // inference proved the parse; the unwrap_or keeps it total.
                    let _ = s.push(*ts, c.parse::<i64>().unwrap_or(0));
                }
                TsColumn::I64(s)
            }
            TsInferredType::F64 => {
                let mut s = TsSeries::<f64>::with_capacity(self.cells.len());
                for (ts, c) in &self.cells {
                    let v = c.parse::<f64>().unwrap_or(f64::NAN);
                    // a parsed-but-non-finite cell (inf/nan token) degrades to a
                    // gap rather than tripping the series' finite-value guard.
                    if v.is_finite() {
                        let _ = s.push(*ts, v);
                    }
                }
                TsColumn::F64(s)
            }
            TsInferredType::Bool => {
                let mut s = TsSeries::<bool>::with_capacity(self.cells.len());
                for (ts, c) in &self.cells {
                    let _ = s.push(*ts, c.eq_ignore_ascii_case("true"));
                }
                TsColumn::Bool(s)
            }
            TsInferredType::Str => {
                let mut s = TsSeries::<String>::with_capacity(self.cells.len());
                for (ts, c) in self.cells {
                    let _ = s.push(ts, c);
                }
                TsColumn::Str(s)
            }
        }
    }
}

/// Assemble a frame from named raw columns in order. Duplicate names from a
/// malformed header are de-duplicated by the frame layer's own rule (the first
/// wins; a later push is skipped) so a read never panics on a repeated header.
pub(crate) fn assemble(names: Vec<String>, raws: Vec<RawColumn>) -> TsDataFrame {
    let mut df = TsDataFrame::new();
    for (name, raw) in names.into_iter().zip(raws) {
        let _ = df.push_column(name, raw.build());
    }
    df
}
