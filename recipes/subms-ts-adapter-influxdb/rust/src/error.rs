//! Error surface for the InfluxDB adapter.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum TsInfluxError {
    /// A line-protocol encode constraint was violated.
    Encode { msg: String },
    /// The annotated-CSV response could not be decoded.
    Csv { msg: String },
    /// The transport reported a non-2xx HTTP status.
    Http { status: u16, body: String },
    /// The transport failed below the HTTP layer (connect, IO, ...).
    Transport { msg: String },
    /// A connection string or config value was malformed.
    Config { msg: String },
}

impl TsInfluxError {
    pub fn encode(msg: impl Into<String>) -> Self {
        Self::Encode { msg: msg.into() }
    }
    pub fn csv(msg: impl Into<String>) -> Self {
        Self::Csv { msg: msg.into() }
    }
    pub fn transport(msg: impl Into<String>) -> Self {
        Self::Transport { msg: msg.into() }
    }
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config { msg: msg.into() }
    }
}

impl fmt::Display for TsInfluxError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Encode { msg } => write!(f, "line-protocol encode error: {msg}"),
            Self::Csv { msg } => write!(f, "csv decode error: {msg}"),
            Self::Http { status, body } => write!(f, "influx http {status}: {body}"),
            Self::Transport { msg } => write!(f, "transport error: {msg}"),
            Self::Config { msg } => write!(f, "config error: {msg}"),
        }
    }
}

impl std::error::Error for TsInfluxError {}
