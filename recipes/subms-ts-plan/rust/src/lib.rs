//! `subms-ts-plan` - compose the per-operation p99 contracts the cookbook
//! recipes already publish into a single system-level latency certificate.
//!
//! Every recipe in the arc asserts a tail-latency budget per stage. On their
//! own those are point facts. A query that prunes with a zone map, decodes a
//! Gorilla block, scans a range, then reads a t-digest quantile runs all four
//! in sequence - so its system p99 is the sum of the constituent p99s plus a
//! planner overhead. `TsPlan` adds them up; `certify` emits a
//! [`TsLatencyCertificate`] an SRE can put in an SLA.
//!
//! The certificate carries a deterministic FNV-1a integrity hash over its
//! canonical JSON, so tampering is detectable and the Rust + Java ports agree
//! byte-for-byte. That hash is tamper-evidence, NOT a cryptographic signature;
//! to sign for real, run a signer over [`TsLatencyCertificate::to_json`] - the
//! signing key is the consumer's, so it stays a pluggable hook here.
//!
//! ```
//! use subms_ts_plan::TsPlan;
//!
//! let cert = TsPlan::new()
//!     .then("subms-zone-map", "candidates", 500_000)
//!     .then("subms-gorilla-block", "range_scan", 37_100)
//!     .then("subms-ts", "range_min", 900)
//!     .then("subms-tdigest", "quantile", 300)
//!     .with_overhead(50_000)
//!     .certify("ci-dedicated", 0);
//! assert!(cert.meets_budget(1_000_000)); // composes to < 1 ms p99
//! assert!(cert.verify());
//! ```

#[cfg(feature = "harness")]
pub mod recipe;

/// One step of a plan: a recipe + stage and the published p99 (ns) for it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsPlanStage {
    pub recipe: String,
    pub stage: String,
    pub p99_ns: u64,
}

/// An ordered sequence of recipe calls plus a flat planner overhead.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TsPlan {
    stages: Vec<TsPlanStage>,
    planner_overhead_ns: u64,
}

impl TsPlan {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append a stage citing a recipe's published p99 (builder style).
    pub fn then(mut self, recipe: impl Into<String>, stage: impl Into<String>, p99_ns: u64) -> Self {
        self.stages.push(TsPlanStage {
            recipe: recipe.into(),
            stage: stage.into(),
            p99_ns,
        });
        self
    }

    /// Flat overhead the planner adds on top of the constituent stages.
    pub fn with_overhead(mut self, ns: u64) -> Self {
        self.planner_overhead_ns = ns;
        self
    }

    pub fn stages(&self) -> &[TsPlanStage] {
        &self.stages
    }

    pub fn planner_overhead_ns(&self) -> u64 {
        self.planner_overhead_ns
    }

    /// Composed system p99: the sum of stage p99s plus the planner overhead.
    /// Saturating so a pathological plan reports `u64::MAX` rather than wrap.
    pub fn total_p99_ns(&self) -> u64 {
        self.stages
            .iter()
            .fold(self.planner_overhead_ns, |acc, s| {
                acc.saturating_add(s.p99_ns)
            })
    }

    /// Freeze the plan into a certificate for `hardware_tier`, valid until the
    /// given epoch-nanos deadline (0 = unbounded).
    pub fn certify(&self, hardware_tier: impl Into<String>, valid_until: i64) -> TsLatencyCertificate {
        let mut cert = TsLatencyCertificate {
            hardware_tier: hardware_tier.into(),
            total_p99_ns: self.total_p99_ns(),
            planner_overhead_ns: self.planner_overhead_ns,
            valid_until,
            stages: self.stages.clone(),
            integrity: 0,
        };
        cert.integrity = fnv1a(cert.canonical_body().as_bytes());
        cert
    }
}

/// A signed-by-checksum latency guarantee composed from a [`TsPlan`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TsLatencyCertificate {
    pub hardware_tier: String,
    pub total_p99_ns: u64,
    pub planner_overhead_ns: u64,
    pub valid_until: i64,
    pub stages: Vec<TsPlanStage>,
    pub integrity: u64,
}

impl TsLatencyCertificate {
    /// Does the composed p99 fit within `budget_ns`?
    pub fn meets_budget(&self, budget_ns: u64) -> bool {
        self.total_p99_ns <= budget_ns
    }

    /// Recompute the integrity hash and compare. Detects any field tamper.
    pub fn verify(&self) -> bool {
        fnv1a(self.canonical_body().as_bytes()) == self.integrity
    }

    /// The canonical JSON of every field except `integrity` - the bytes the
    /// hash is taken over and what a real signer would sign.
    fn canonical_body(&self) -> String {
        let mut out = String::new();
        out.push('{');
        out.push_str("\"hardware_tier\":");
        push_json_str(&mut out, &self.hardware_tier);
        out.push_str(&format!(",\"total_p99_ns\":{}", self.total_p99_ns));
        out.push_str(&format!(",\"planner_overhead_ns\":{}", self.planner_overhead_ns));
        out.push_str(&format!(",\"valid_until\":{}", self.valid_until));
        out.push_str(",\"stages\":[");
        for (i, s) in self.stages.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"recipe\":");
            push_json_str(&mut out, &s.recipe);
            out.push_str(",\"stage\":");
            push_json_str(&mut out, &s.stage);
            out.push_str(&format!(",\"p99_ns\":{}}}", s.p99_ns));
        }
        out.push_str("]}");
        out
    }

    /// Full certificate JSON, including the integrity hash.
    pub fn to_json(&self) -> String {
        let body = self.canonical_body();
        // splice `,"integrity":N` before the closing brace of the body.
        let mut out = body[..body.len() - 1].to_string();
        out.push_str(&format!(",\"integrity\":{}}}", self.integrity));
        out
    }
}

fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
}

/// FNV-1a 64-bit. Deterministic across languages over the same UTF-8 bytes, so
/// the certificate hash is byte-equivalent on Rust + Java.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}
