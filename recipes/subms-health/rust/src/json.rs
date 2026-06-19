//! Hand-rolled deterministic JSON. No serde. The byte layout here is the
//! cross-language contract: the Java and Python ports emit the same bytes for
//! the same inputs (a fixture pins it in every test suite).

use std::collections::BTreeMap;

use crate::component::ComponentHealth;

/// The only value type allowed in a component's `details`. Kept small so the
/// serialisation stays byte-stable across languages. Floats are accepted but
/// excluded from the cross-language byte guarantee (formatting differs across
/// runtimes); the default env/deploy sections only ever emit string/bool/int.
#[derive(Debug, Clone, PartialEq)]
pub enum JsonValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Str(String),
}

impl From<&str> for JsonValue {
    fn from(s: &str) -> Self {
        JsonValue::Str(s.to_string())
    }
}
impl From<String> for JsonValue {
    fn from(s: String) -> Self {
        JsonValue::Str(s)
    }
}
impl From<bool> for JsonValue {
    fn from(b: bool) -> Self {
        JsonValue::Bool(b)
    }
}
impl From<i64> for JsonValue {
    fn from(n: i64) -> Self {
        JsonValue::Int(n)
    }
}
impl From<u64> for JsonValue {
    fn from(n: u64) -> Self {
        JsonValue::Int(n as i64)
    }
}
impl From<i32> for JsonValue {
    fn from(n: i32) -> Self {
        JsonValue::Int(n as i64)
    }
}
impl From<u32> for JsonValue {
    fn from(n: u32) -> Self {
        JsonValue::Int(n as i64)
    }
}
impl From<usize> for JsonValue {
    fn from(n: usize) -> Self {
        JsonValue::Int(n as i64)
    }
}
impl From<f64> for JsonValue {
    fn from(n: f64) -> Self {
        JsonValue::Float(n)
    }
}

/// String escaping matched to Python's `json.dumps` (and our Java port): the
/// named escapes plus `\u00xx` for the remaining control chars.
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

pub(crate) fn push_value(out: &mut String, v: &JsonValue) {
    match v {
        JsonValue::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        JsonValue::Int(n) => out.push_str(&n.to_string()),
        JsonValue::Float(f) => out.push_str(&f.to_string()),
        JsonValue::Str(s) => push_json_str(out, s),
    }
}

/// `{"k":v,...}` with keys in sorted (BTreeMap) order.
pub(crate) fn push_map(out: &mut String, m: &BTreeMap<String, JsonValue>) {
    out.push('{');
    for (i, (k, v)) in m.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_str(out, k);
        out.push(':');
        push_value(out, v);
    }
    out.push('}');
}

/// A bare component: `{"status":"UP"[,"details":{...}][,"components":{...}]}`.
/// Empty `details` / `components` are omitted.
pub(crate) fn push_component(out: &mut String, c: &ComponentHealth) {
    out.push('{');
    out.push_str("\"status\":");
    push_json_str(out, c.status.as_str());
    if !c.details.is_empty() {
        out.push_str(",\"details\":");
        push_map(out, &c.details);
    }
    if !c.components.is_empty() {
        out.push_str(",\"components\":");
        push_component_map(out, &c.components);
    }
    out.push('}');
}

/// `{"<name>": <component>, ...}` with names in sorted order.
pub(crate) fn push_component_map(out: &mut String, m: &BTreeMap<String, ComponentHealth>) {
    out.push('{');
    for (i, (k, v)) in m.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        push_json_str(out, k);
        out.push(':');
        push_component(out, v);
    }
    out.push('}');
}

/// FNV-1a 64-bit over UTF-8 bytes. Deterministic across languages, so the
/// secret hash/fingerprint redaction reads identically on every port.
pub(crate) fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
