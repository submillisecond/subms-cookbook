//! Env/deploy provider. A named section selects environment variables by an
//! explicit key list plus prefix/glob patterns, applies redaction + remapping,
//! and renders as a `ComponentHealth` whose `details` are the matched vars. This
//! is the part no JVM health framework ships out of the box.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use crate::component::{ComponentHealth, HealthIndicator};
use crate::json::{JsonValue, fnv1a};
use crate::status::HealthStatus;

/// Source of environment variables. Injectable so tests can freeze the env.
pub trait EnvProvider: Send + Sync {
    fn get(&self, key: &str) -> Option<String>;
    fn keys(&self) -> Vec<String>;
}

/// Reads the real process environment.
pub struct SystemEnv;

impl EnvProvider for SystemEnv {
    fn get(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
    fn keys(&self) -> Vec<String> {
        std::env::vars().map(|(k, _)| k).collect()
    }
}

/// In-memory env for tests / frozen-at-boot snapshots.
#[derive(Default)]
pub struct MapEnv {
    vars: BTreeMap<String, String>,
}

impl MapEnv {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn with(mut self, key: &str, value: &str) -> Self {
        self.vars.insert(key.to_string(), value.to_string());
        self
    }
}

impl EnvProvider for MapEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.vars.get(key).cloned()
    }
    fn keys(&self) -> Vec<String> {
        self.vars.keys().cloned().collect()
    }
}

/// How to mask a matched secret. Hash/Fingerprint use FNV-1a so two deploys can
/// be confirmed to carry the same secret without revealing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionPolicy {
    /// Replace with `***`.
    Mask,
    /// `***` + the last 4 chars (or `***` if shorter than 5).
    Last4,
    /// `fnv1a:<16 hex>` of the value.
    Hash,
    /// `fp_<6 hex>` short fingerprint of the value.
    Fingerprint,
}

impl RedactionPolicy {
    pub fn apply(self, value: &str) -> String {
        match self {
            RedactionPolicy::Mask => "***".to_string(),
            RedactionPolicy::Last4 => {
                let chars: Vec<char> = value.chars().collect();
                if chars.len() <= 4 {
                    "***".to_string()
                } else {
                    let last: String = chars[chars.len() - 4..].iter().collect();
                    format!("***{last}")
                }
            }
            RedactionPolicy::Hash => format!("fnv1a:{:016x}", fnv1a(value.as_bytes())),
            RedactionPolicy::Fingerprint => {
                format!("fp_{:06x}", fnv1a(value.as_bytes()) & 0xff_ffff)
            }
        }
    }
}

const SECRET_NEEDLES: &[&str] = &["SECRET", "TOKEN", "KEY", "PASSWORD", "PASS", "CREDENTIAL"];

/// Declarative env section. Build it fluently, then `render` it against a
/// provider or turn it into a `HealthIndicator`.
pub struct EnvSection {
    name: String,
    explicit: Vec<String>,
    prefixes: Vec<String>,
    globs: Vec<String>,
    redactions: Vec<(String, RedactionPolicy)>,
    redact_substrings: Vec<(String, RedactionPolicy)>,
    remap: BTreeMap<String, String>,
    strip_prefix_in_key: bool,
    lowercase_keys: bool,
    status: HealthStatus,
    include_empty: bool,
}

impl EnvSection {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            explicit: Vec::new(),
            prefixes: Vec::new(),
            globs: Vec::new(),
            redactions: Vec::new(),
            redact_substrings: Vec::new(),
            remap: BTreeMap::new(),
            strip_prefix_in_key: false,
            lowercase_keys: false,
            status: HealthStatus::Up,
            include_empty: false,
        }
    }

    /// Include one exact key.
    pub fn key(mut self, key: &str) -> Self {
        self.explicit.push(key.to_string());
        self
    }

    /// Include several exact keys.
    pub fn keys<I, S>(mut self, keys: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        for k in keys {
            self.explicit.push(k.into());
        }
        self
    }

    /// Include every key starting with `prefix`.
    pub fn prefix(mut self, prefix: &str) -> Self {
        self.prefixes.push(prefix.to_string());
        self
    }

    /// Include every key matching a glob with a single `*` (prefix/suffix/middle).
    pub fn glob(mut self, pattern: &str) -> Self {
        self.globs.push(pattern.to_string());
        self
    }

    /// Redact a specific detail/raw key with a policy.
    pub fn redact(mut self, key: &str, policy: RedactionPolicy) -> Self {
        self.redactions.push((key.to_string(), policy));
        self
    }

    /// Redact any key (raw or detail) containing `needle` (case-insensitive).
    pub fn redact_substring(mut self, needle: &str, policy: RedactionPolicy) -> Self {
        self.redact_substrings.push((needle.to_string(), policy));
        self
    }

    /// Mask anything that looks like a secret (SECRET/TOKEN/KEY/PASSWORD/PASS/CREDENTIAL).
    pub fn redact_secrets(mut self) -> Self {
        for n in SECRET_NEEDLES {
            self.redact_substrings
                .push(((*n).to_string(), RedactionPolicy::Mask));
        }
        self
    }

    /// Rename a raw key to a detail key.
    pub fn remap(mut self, from: &str, to: &str) -> Self {
        self.remap.insert(from.to_string(), to.to_string());
        self
    }

    /// Strip the matched prefix from the detail key (e.g. `KICKSTART_ENV` -> `ENV`).
    pub fn strip_prefix_in_key(mut self, yes: bool) -> Self {
        self.strip_prefix_in_key = yes;
        self
    }

    /// Lower-case detail keys.
    pub fn lowercase_keys(mut self, yes: bool) -> Self {
        self.lowercase_keys = yes;
        self
    }

    /// Section status (default Up - the section is informational).
    pub fn status(mut self, status: HealthStatus) -> Self {
        self.status = status;
        self
    }

    /// Include vars that are set but empty (default: skipped).
    pub fn include_empty(mut self, yes: bool) -> Self {
        self.include_empty = yes;
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    fn matches(&self, key: &str) -> bool {
        self.explicit.iter().any(|k| k == key)
            || self.prefixes.iter().any(|p| key.starts_with(p.as_str()))
            || self.globs.iter().any(|g| glob_match(g, key))
    }

    fn detail_key(&self, raw: &str) -> String {
        if let Some(r) = self.remap.get(raw) {
            return r.clone();
        }
        let mut dk = raw.to_string();
        if self.strip_prefix_in_key {
            let mut best = "";
            for p in &self.prefixes {
                if raw.starts_with(p.as_str()) && p.len() > best.len() {
                    best = p;
                }
            }
            if !best.is_empty() {
                dk = raw[best.len()..].to_string();
            }
        }
        if self.lowercase_keys {
            dk = dk.to_lowercase();
        }
        dk
    }

    fn policy_for(&self, raw: &str, detail: &str) -> Option<RedactionPolicy> {
        for (k, p) in &self.redactions {
            if k == raw || k == detail {
                return Some(*p);
            }
        }
        let rl = raw.to_lowercase();
        let dl = detail.to_lowercase();
        for (s, p) in &self.redact_substrings {
            let sl = s.to_lowercase();
            if rl.contains(&sl) || dl.contains(&sl) {
                return Some(*p);
            }
        }
        None
    }

    /// Resolve the section against a provider into a `ComponentHealth`.
    pub fn render(&self, env: &dyn EnvProvider) -> ComponentHealth {
        let mut candidates: BTreeSet<String> = BTreeSet::new();
        for k in &self.explicit {
            candidates.insert(k.clone());
        }
        for k in env.keys() {
            if self.matches(&k) {
                candidates.insert(k);
            }
        }
        let mut details: BTreeMap<String, JsonValue> = BTreeMap::new();
        for raw in candidates {
            let val = match env.get(&raw) {
                Some(v) => v,
                None => continue,
            };
            if val.is_empty() && !self.include_empty {
                continue;
            }
            let dk = self.detail_key(&raw);
            let out = match self.policy_for(&raw, &dk) {
                Some(p) => p.apply(&val),
                None => val,
            };
            details.insert(dk, JsonValue::Str(out));
        }
        ComponentHealth {
            status: self.status,
            details,
            components: BTreeMap::new(),
        }
    }

    /// Bind the section to a provider as a `HealthIndicator`.
    pub fn into_indicator(self, env: Arc<dyn EnvProvider>) -> EnvSectionIndicator {
        EnvSectionIndicator { section: self, env }
    }

    /// Bind to the real process environment.
    pub fn into_system_indicator(self) -> EnvSectionIndicator {
        self.into_indicator(Arc::new(SystemEnv))
    }
}

/// An `EnvSection` bound to a provider, usable as a `HealthIndicator`.
pub struct EnvSectionIndicator {
    section: EnvSection,
    env: Arc<dyn EnvProvider>,
}

impl HealthIndicator for EnvSectionIndicator {
    fn name(&self) -> &str {
        &self.section.name
    }
    fn check(&self) -> ComponentHealth {
        self.section.render(self.env.as_ref())
    }
}

/// Glob with at most one `*` (prefix, suffix, or middle wildcard).
fn glob_match(pat: &str, s: &str) -> bool {
    match pat.find('*') {
        None => pat == s,
        Some(i) => {
            let pre = &pat[..i];
            let post = &pat[i + 1..];
            s.len() >= pre.len() + post.len() && s.starts_with(pre) && s.ends_with(post)
        }
    }
}
