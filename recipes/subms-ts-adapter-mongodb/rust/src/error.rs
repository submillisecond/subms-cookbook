//! Error surface for the MongoDB adapter.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum TsMongoError {
    /// A document did not carry the shape the mapping expects.
    Mapping { msg: String },
    /// BSON encode / decode failed.
    Bson { msg: String },
    /// The underlying store reported a failure (driver IO, server error...).
    Store { msg: String },
    /// A connection string or config value was malformed.
    Config { msg: String },
}

impl TsMongoError {
    pub fn mapping(msg: impl Into<String>) -> Self {
        Self::Mapping { msg: msg.into() }
    }
    pub fn bson(msg: impl Into<String>) -> Self {
        Self::Bson { msg: msg.into() }
    }
    pub fn store(msg: impl Into<String>) -> Self {
        Self::Store { msg: msg.into() }
    }
    pub fn config(msg: impl Into<String>) -> Self {
        Self::Config { msg: msg.into() }
    }
}

impl fmt::Display for TsMongoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mapping { msg } => write!(f, "mongo mapping error: {msg}"),
            Self::Bson { msg } => write!(f, "bson error: {msg}"),
            Self::Store { msg } => write!(f, "mongo store error: {msg}"),
            Self::Config { msg } => write!(f, "config error: {msg}"),
        }
    }
}

impl std::error::Error for TsMongoError {}
