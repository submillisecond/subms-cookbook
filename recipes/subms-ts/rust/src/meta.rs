//! Per-series identity, schema, tags, normalised attributes, declared wire
//! format, and downstream-dependency edges. Metadata is optional - a bare
//! `TsSeries` carries none, and the sub-ms ingest path never touches it.

use std::collections::BTreeMap;

/// Prometheus-style label set. Case-sensitive (labels are identifiers).
pub type TsTags = BTreeMap<String, String>;

/// Semantic shape of a series' values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum TsSchema {
    #[default]
    Anonymous,
    Numeric {
        unit: Option<String>,
        kind: TsNumericKind,
    },
    Schemaless,
    Custom {
        type_name: String,
    },
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum TsNumericKind {
    Gauge,
    Counter,
    Rate,
}

/// Declared preferred wire format. Lets a container pick a codec at serialize
/// time without inspecting the value shape; also records "what did this load
/// as" when materialising from disk.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsFormat {
    Json,
    Cbor,
    Gorilla,
    Yaml,
    GzipJson,
    GzipCbor,
    GzipGorilla,
    Custom(String),
}

impl TsFormat {
    pub fn codec_name(&self) -> &str {
        match self {
            TsFormat::Json => "json",
            TsFormat::Cbor => "cbor",
            TsFormat::Gorilla => "gorilla",
            TsFormat::Yaml => "yaml",
            TsFormat::GzipJson => "gzip+json",
            TsFormat::GzipCbor => "gzip+cbor",
            TsFormat::GzipGorilla => "gzip+gorilla",
            TsFormat::Custom(s) => s,
        }
    }
}

/// How a derived series relates to a target series in the same container.
/// Lets a planner walk the dep graph: "what would invalidating trades.aapl
/// invalidate?" -> everything declaring it as a `Derived` / `Aggregate` dep.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsDepKind {
    Derived,
    Component,
    Aggregate,
    AsofJoinLeft,
    AsofJoinRight,
    Custom(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsDep {
    pub series_id: u64,
    pub kind: TsDepKind,
    pub note: Option<String>,
}

impl TsDep {
    pub fn new(series_id: u64, kind: TsDepKind) -> Self {
        Self {
            series_id,
            kind,
            note: None,
        }
    }

    pub fn with_note(mut self, note: impl Into<String>) -> Self {
        self.note = Some(note.into());
        self
    }
}

/// Free-form attribute bag with canonicalising inserts: key + value are
/// trimmed and ASCII-lowercased so query-by-attribute is canonical without
/// the caller doing string hygiene. Non-ASCII keys are rejected (returned as
/// the `Err` value) rather than mangled by ambiguous Unicode case folding.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsAttrs(BTreeMap<String, String>);

impl TsAttrs {
    pub fn new() -> Self {
        Self(BTreeMap::new())
    }

    /// Insert a normalised key=value. Returns `Err(key)` if the key contains
    /// non-ASCII bytes (case folding is ambiguous there - reject, don't guess).
    pub fn insert(&mut self, key: impl AsRef<str>, value: impl AsRef<str>) -> Result<(), String> {
        let key = key.as_ref().trim();
        if !key.is_ascii() {
            return Err(key.to_string());
        }
        let k = key.to_ascii_lowercase();
        let v = value.as_ref().trim().to_ascii_lowercase();
        self.0.insert(k, v);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.0
            .get(&key.trim().to_ascii_lowercase())
            .map(|s| s.as_str())
    }

    pub fn remove(&mut self, key: &str) -> Option<String> {
        self.0.remove(&key.trim().to_ascii_lowercase())
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Does this attribute set match `key=value` after normalising both?
    pub fn matches(&self, key: &str, value: &str) -> bool {
        self.get(key) == Some(value.trim().to_ascii_lowercase().as_str())
    }
}

/// Identity + schema + relationship block for a series. Optional on a
/// `TsSeries`; required when the series lives in a `TsCollection` registry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TsSeriesMetadata {
    pub id: u64,
    pub name: String,
    pub schema: TsSchema,
    pub format: Option<TsFormat>,
    pub tags: TsTags,
    pub attributes: TsAttrs,
    pub dependencies: Vec<TsDep>,
}

impl TsSeriesMetadata {
    pub fn new(id: u64, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
            ..Default::default()
        }
    }

    pub fn with_schema(mut self, schema: TsSchema) -> Self {
        self.schema = schema;
        self
    }

    pub fn with_format(mut self, format: TsFormat) -> Self {
        self.format = Some(format);
        self
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }

    pub fn with_dependency(mut self, dep: TsDep) -> Self {
        self.dependencies.push(dep);
        self
    }

    /// True if every `key=value` in `tags` matches this series' tags.
    pub fn has_tags(&self, tags: &TsTags) -> bool {
        tags.iter().all(|(k, v)| self.tags.get(k) == Some(v))
    }
}
