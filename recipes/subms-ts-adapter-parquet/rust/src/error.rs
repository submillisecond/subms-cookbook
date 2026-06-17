//! Error surface for the Parquet adapter.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum TsParquetError {
    /// The Arrow bridge (series <-> RecordBatch) reported a failure.
    Arrow { msg: String },
    /// Parquet encode / decode failed.
    Parquet { msg: String },
    /// A file held no row groups.
    Empty,
}

impl TsParquetError {
    pub fn arrow(msg: impl Into<String>) -> Self {
        Self::Arrow { msg: msg.into() }
    }
    pub fn parquet(msg: impl Into<String>) -> Self {
        Self::Parquet { msg: msg.into() }
    }
}

impl fmt::Display for TsParquetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Arrow { msg } => write!(f, "arrow bridge error: {msg}"),
            Self::Parquet { msg } => write!(f, "parquet error: {msg}"),
            Self::Empty => write!(f, "parquet file held no row groups"),
        }
    }
}

impl std::error::Error for TsParquetError {}
