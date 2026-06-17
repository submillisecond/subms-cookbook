//! Error surface for the Arrow adapter.

use std::fmt;

#[derive(Clone, Debug, PartialEq)]
pub enum TsArrowError {
    /// A batch did not carry the columns / types the mapping expects.
    Mapping { msg: String },
    /// The Arrow layer (batch build, schema) reported a failure.
    Arrow { msg: String },
    /// IPC stream read / write failed.
    Ipc { msg: String },
}

impl TsArrowError {
    pub fn mapping(msg: impl Into<String>) -> Self {
        Self::Mapping { msg: msg.into() }
    }
    pub fn arrow(msg: impl Into<String>) -> Self {
        Self::Arrow { msg: msg.into() }
    }
    pub fn ipc(msg: impl Into<String>) -> Self {
        Self::Ipc { msg: msg.into() }
    }
}

impl fmt::Display for TsArrowError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Mapping { msg } => write!(f, "arrow mapping error: {msg}"),
            Self::Arrow { msg } => write!(f, "arrow error: {msg}"),
            Self::Ipc { msg } => write!(f, "arrow ipc error: {msg}"),
        }
    }
}

impl std::error::Error for TsArrowError {}
