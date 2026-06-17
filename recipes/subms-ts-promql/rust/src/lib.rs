//! `subms-ts-promql` - a zero-dependency, hand-rolled PromQL-subset query
//! engine over a [`TsCollection<f64>`]. It parses a useful slice of the
//! PromQL surface (instant + range selectors with label matchers, the
//! `sum`/`avg`/`min`/`max`/`count` aggregations with `by`/`without` grouping,
//! the `rate`/`irate`/`increase` range functions, scalar/vector binary ops,
//! and the `offset` modifier) and evaluates it against the series in a
//! collection, resolving a selector to the set of series whose metadata name +
//! label tags match.
//!
//! The whole thing is std-only: a byte-cursor lexer, a precedence-climbing
//! recursive-descent parser, and a tree-walking evaluator. There is no regex
//! crate - `=~` / `!~` go through a deliberately small anchored matcher that
//! understands literal text and the `.*` wildcard, and nothing else. The
//! [non-claims](#non-claims) below spell out exactly what is and is not in.
//!
//! ```
//! use subms_ts::{TsCollection, TsSeriesMetadata};
//! use subms_ts_promql::TsPromQl;
//!
//! let mut coll = TsCollection::<f64>::new();
//! let id = coll
//!     .register(TsSeriesMetadata::new(1, "http_requests").with_tag("job", "api"))
//!     .unwrap();
//! coll.push(id, 1_000, 10.0).unwrap();
//! coll.push(id, 2_000, 12.0).unwrap();
//!
//! let engine = TsPromQl::new(&coll);
//! let res = engine.eval_instant("http_requests{job=\"api\"}", 2_000).unwrap();
//! assert_eq!(res.len(), 1);
//! assert_eq!(res.samples()[0].value, 12.0);
//! ```
//!
//! # Non-claims
//!
//! This is the common-query subset, not all of PromQL. Out of scope:
//! subqueries, the `@` modifier, `histogram_quantile` and the rest of the
//! function library, full PCRE `=~` (only literal + `.*`), staleness
//! handling, `bool` modifiers, `on`/`ignoring`/`group_left` vector matching,
//! and `topk`/`bottomk`/`quantile` aggregations.

use subms_ts::{TsCollection, TsTags};

pub mod eval;
pub mod parser;

#[cfg(feature = "harness")]
pub mod recipe;

/// A single result sample: the surviving label set paired with its value at
/// the eval instant. The PromQL "instant vector" element.
#[derive(Clone, Debug, PartialEq)]
pub struct TsSample {
    pub labels: TsTags,
    pub value: f64,
}

/// The result of evaluating a query at an instant: a vector of samples.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TsPromQlResult {
    samples: Vec<TsSample>,
}

impl TsPromQlResult {
    pub fn new(samples: Vec<TsSample>) -> Self {
        Self { samples }
    }

    pub fn samples(&self) -> &[TsSample] {
        &self.samples
    }

    pub fn into_samples(self) -> Vec<TsSample> {
        self.samples
    }

    pub fn len(&self) -> usize {
        self.samples.len()
    }

    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The value of the single sample whose labels match `want` exactly, if
    /// the result holds exactly that one matching sample. Handy in tests.
    pub fn value_for(&self, want: &TsTags) -> Option<f64> {
        self.samples
            .iter()
            .find(|s| &s.labels == want)
            .map(|s| s.value)
    }

    /// The lone value when the result is a single sample (scalar or one-series
    /// vector). `None` for empty or multi-sample results.
    pub fn scalar(&self) -> Option<f64> {
        match self.samples.as_slice() {
            [s] => Some(s.value),
            _ => None,
        }
    }
}

/// One `(instant, sample-vector)` step of a range evaluation.
#[derive(Clone, Debug, PartialEq)]
pub struct TsRangeStep {
    pub ts: i64,
    pub result: TsPromQlResult,
}

/// A range evaluation: the query evaluated at each step across `[start, end]`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct TsPromQlRangeResult {
    steps: Vec<TsRangeStep>,
}

impl TsPromQlRangeResult {
    pub fn steps(&self) -> &[TsRangeStep] {
        &self.steps
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

/// What can go wrong: a malformed query is a [`TsPromQlError::Parse`]; a query
/// that parses but cannot be evaluated as written is a [`TsPromQlError::Eval`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TsPromQlError {
    Parse(String),
    Eval(String),
}

impl std::fmt::Display for TsPromQlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsPromQlError::Parse(m) => write!(f, "promql parse error: {m}"),
            TsPromQlError::Eval(m) => write!(f, "promql eval error: {m}"),
        }
    }
}

impl std::error::Error for TsPromQlError {}

/// The query engine: a read-only view over a [`TsCollection<f64>`]. Cheap to
/// construct; holds only a borrow, so one collection can back many engines.
pub struct TsPromQl<'a> {
    coll: &'a TsCollection<f64>,
}

impl<'a> TsPromQl<'a> {
    pub fn new(coll: &'a TsCollection<f64>) -> Self {
        Self { coll }
    }

    /// Parse + evaluate `query` at the instant `at_ts` (i64 nanos). The result
    /// is an instant vector of `(label set, value)` samples.
    pub fn eval_instant(&self, query: &str, at_ts: i64) -> Result<TsPromQlResult, TsPromQlError> {
        let expr = parser::parse(query)?;
        eval::eval_instant(self.coll, &expr, at_ts)
    }

    /// Parse once, evaluate at every step in `[start, end]` advancing by `step`
    /// nanos. `step` must be positive and `start <= end`.
    pub fn eval_range(
        &self,
        query: &str,
        start: i64,
        end: i64,
        step: i64,
    ) -> Result<TsPromQlRangeResult, TsPromQlError> {
        if step <= 0 {
            return Err(TsPromQlError::Eval("range step must be positive".into()));
        }
        if start > end {
            return Err(TsPromQlError::Eval("range start is after end".into()));
        }
        let expr = parser::parse(query)?;
        let mut steps = Vec::new();
        let mut at = start;
        while at <= end {
            let result = eval::eval_instant(self.coll, &expr, at)?;
            steps.push(TsRangeStep { ts: at, result });
            // checked step avoids an infinite loop if at+step overflows.
            match at.checked_add(step) {
                Some(next) => at = next,
                None => break,
            }
        }
        Ok(TsPromQlRangeResult { steps })
    }
}
