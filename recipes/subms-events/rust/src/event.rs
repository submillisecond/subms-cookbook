//! The structured `Event` value, its severity `EventLevel`, and a fluent
//! `EventBuilder`. JSON is hand-rolled and deterministic - the byte layout is the
//! cross-language contract (a fixture pins it on every port).

use std::collections::BTreeMap;

/// Event severity. Wire tokens are UPPERCASE.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EventLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl EventLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            EventLevel::Trace => "TRACE",
            EventLevel::Debug => "DEBUG",
            EventLevel::Info => "INFO",
            EventLevel::Warn => "WARN",
            EventLevel::Error => "ERROR",
        }
    }
}

/// A structured, immutable event: a topic, a level, an optional timestamp +
/// message, and a sorted string attribute map. Cheap to clone (it crosses the
/// dispatcher channel by value in async mode).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub topic: String,
    pub level: EventLevel,
    pub at: String,
    pub message: Option<String>,
    pub attributes: BTreeMap<String, String>,
}

impl Event {
    /// Start a builder for `topic` (default level Info).
    pub fn builder(topic: &str) -> EventBuilder {
        EventBuilder::new(topic)
    }

    /// Convenience for the common "X moved A -> B" shape: topic + `scope`/`from`/`to`
    /// attributes. The caller picks the level (a Down transition is an Error, an
    /// Up recovery is Info, etc.).
    pub fn transition(topic: &str, level: EventLevel, scope: &str, from: &str, to: &str) -> Event {
        EventBuilder::new(topic)
            .level(level)
            .attr("scope", scope)
            .attr("from", from)
            .attr("to", to)
            .build()
    }

    /// Read an attribute.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes.get(key).map(|s| s.as_str())
    }

    /// Deterministic JSON: `{"topic":..,"level":"INFO","at":..[,"message":..][,"attributes":{sorted}]}`.
    /// Empty `at` is still emitted (it is a required field); `message` /
    /// `attributes` are omitted when absent/empty.
    pub fn to_json(&self) -> String {
        let mut out = String::new();
        out.push_str("{\"topic\":");
        push_json_str(&mut out, &self.topic);
        out.push_str(",\"level\":");
        push_json_str(&mut out, self.level.as_str());
        out.push_str(",\"at\":");
        push_json_str(&mut out, &self.at);
        if let Some(m) = &self.message {
            out.push_str(",\"message\":");
            push_json_str(&mut out, m);
        }
        if !self.attributes.is_empty() {
            out.push_str(",\"attributes\":{");
            for (i, (k, v)) in self.attributes.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                push_json_str(&mut out, k);
                out.push(':');
                push_json_str(&mut out, v);
            }
            out.push('}');
        }
        out.push('}');
        out
    }
}

/// Fluent builder for [`Event`].
pub struct EventBuilder {
    topic: String,
    level: EventLevel,
    at: String,
    message: Option<String>,
    attributes: BTreeMap<String, String>,
}

impl EventBuilder {
    pub fn new(topic: &str) -> Self {
        Self {
            topic: topic.to_string(),
            level: EventLevel::Info,
            at: String::new(),
            message: None,
            attributes: BTreeMap::new(),
        }
    }
    pub fn level(mut self, level: EventLevel) -> Self {
        self.level = level;
        self
    }
    pub fn at(mut self, at: &str) -> Self {
        self.at = at.to_string();
        self
    }
    pub fn message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }
    pub fn attr(mut self, key: &str, value: &str) -> Self {
        self.attributes.insert(key.to_string(), value.to_string());
        self
    }
    pub fn build(self) -> Event {
        Event {
            topic: self.topic,
            level: self.level,
            at: self.at,
            message: self.message,
            attributes: self.attributes,
        }
    }
}

/// JSON string escaping matched to Python's `json.dumps` and the Java port.
pub(crate) fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
}
