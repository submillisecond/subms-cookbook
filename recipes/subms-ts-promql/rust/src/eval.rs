//! Evaluator: walk a parsed [`Expr`] against a [`TsCollection<f64>`] at an
//! instant (or over a stepped range), resolving selectors to the matching
//! series by metric name + label matchers over each series' `TsTags`.
//!
//! A selector resolves to the set of series whose metadata `name` equals the
//! metric and whose tags satisfy every matcher. Instant evaluation reads each
//! matched series' `nearest_before(at)` value; range functions read the series'
//! `[at-range, at]` window. The result of any expression is a vector of
//! `(label set, value)` samples - PromQL's instant vector.

use std::collections::BTreeMap;

use subms_ts::{TsCollection, TsSeries, TsTags};

use crate::parser::{AggOp, BinOp, Expr, FuncKind, Grouping, Selector};
use crate::{TsPromQlError, TsPromQlResult, TsSample};

/// Evaluate `expr` against `coll` at instant `at` (i64 nanos). Returns an
/// instant vector. Scalars surface as a single sample with an empty label set.
pub fn eval_instant(
    coll: &TsCollection<f64>,
    expr: &Expr,
    at: i64,
) -> Result<TsPromQlResult, TsPromQlError> {
    let samples = eval(coll, expr, at)?;
    Ok(TsPromQlResult::new(samples))
}

fn eval(coll: &TsCollection<f64>, expr: &Expr, at: i64) -> Result<Vec<TsSample>, TsPromQlError> {
    match expr {
        Expr::Scalar(v) => Ok(vec![TsSample {
            labels: TsTags::new(),
            value: *v,
        }]),
        Expr::Selector(sel) => Ok(eval_selector_instant(coll, sel, at)),
        Expr::Func {
            kind,
            selector,
            range_ns,
        } => Ok(eval_func(coll, *kind, selector, *range_ns, at)),
        Expr::Agg {
            op,
            grouping,
            inner,
        } => {
            let inner = eval(coll, inner, at)?;
            Ok(eval_agg(*op, grouping, inner))
        }
        Expr::Binary { op, lhs, rhs } => {
            let l = eval(coll, lhs, at)?;
            let r = eval(coll, rhs, at)?;
            eval_binary(*op, l, r)
        }
    }
}

/// The reserved label that carries a series' metric name, mirroring
/// Prometheus' `__name__`. A selector's bare metric matches against this label
/// if present, falling back to the series' metadata `name`. Storing the metric
/// in a label is what lets many series share one metric name (distinguished by
/// their other labels) inside a single [`TsCollection`], whose registry would
/// otherwise reject a duplicate `name`.
pub const METRIC_LABEL: &str = "__name__";

/// Series matched by a selector, in deterministic order (sorted by id) so the
/// result vector is stable across runs despite the collection's hash storage.
fn matched_series<'a>(coll: &'a TsCollection<f64>, sel: &Selector) -> Vec<&'a TsSeries<f64>> {
    let mut out: Vec<(u64, &TsSeries<f64>)> = coll
        .series()
        .filter_map(|s| {
            let m = s.metadata()?;
            let metric = m
                .tags
                .get(METRIC_LABEL)
                .map(|s| s.as_str())
                .unwrap_or(m.name.as_str());
            if metric != sel.metric {
                return None;
            }
            let ok = sel
                .matchers
                .iter()
                .all(|matcher| matcher.matches(m.tags.get(&matcher.label).map(|s| s.as_str())));
            if ok { Some((m.id, s)) } else { None }
        })
        .collect();
    out.sort_by_key(|(id, _)| *id);
    out.into_iter().map(|(_, s)| s).collect()
}

fn eval_selector_instant(coll: &TsCollection<f64>, sel: &Selector, at: i64) -> Vec<TsSample> {
    let probe = at.saturating_sub(sel.offset_ns);
    matched_series(coll, sel)
        .into_iter()
        .filter_map(|s| {
            let p = s.nearest_before(probe)?;
            Some(TsSample {
                labels: output_labels(s),
                value: p.value,
            })
        })
        .collect()
}

/// A series' label set with the reserved `__name__` stripped - PromQL drops
/// the metric label from a selector's output samples.
fn output_labels(s: &TsSeries<f64>) -> TsTags {
    let mut t = s.metadata().map(|m| m.tags.clone()).unwrap_or_default();
    t.remove(METRIC_LABEL);
    t
}

fn eval_func(
    coll: &TsCollection<f64>,
    kind: FuncKind,
    sel: &Selector,
    range_ns: i64,
    at: i64,
) -> Vec<TsSample> {
    let hi = at.saturating_sub(sel.offset_ns);
    let lo = hi.saturating_sub(range_ns);
    matched_series(coll, sel)
        .into_iter()
        .filter_map(|s| {
            let pts: Vec<(i64, f64)> = s.range(lo, hi).map(|p| (p.ts, p.value)).collect();
            let value = match kind {
                FuncKind::Rate => counter_rate(&pts, range_ns),
                FuncKind::Increase => counter_increase(&pts),
                FuncKind::Irate => counter_irate(&pts),
            }?;
            Some(TsSample {
                labels: output_labels(s),
                value,
            })
        })
        .collect()
}

/// Per-second rate over a counter range. Sums the positive deltas between
/// consecutive samples (treating any negative delta as a counter reset, the
/// value before the drop being the pre-reset total) and divides by the elapsed
/// time between the first and last sample in the window. Dividing by the
/// observed sample span (rather than the nominal range width) yields the true
/// per-second slope; it is a simpler model than Prometheus' edge
/// extrapolation but agrees with it when samples cover the window evenly.
fn counter_rate(pts: &[(i64, f64)], range_ns: i64) -> Option<f64> {
    if pts.len() < 2 || range_ns <= 0 {
        return None;
    }
    let span_ns = pts[pts.len() - 1].0 - pts[0].0;
    if span_ns <= 0 {
        return None;
    }
    let delta = counter_delta(pts);
    let seconds = span_ns as f64 / 1_000_000_000.0;
    Some(delta / seconds)
}

/// `increase` is `rate * range_seconds` - the total counter growth over the
/// window, reset-corrected. Same delta, no division.
fn counter_increase(pts: &[(i64, f64)]) -> Option<f64> {
    if pts.len() < 2 {
        return None;
    }
    Some(counter_delta(pts))
}

/// `irate` is the per-second rate of the last two samples only - the
/// instantaneous rate. Negative step is treated as a reset (delta = last).
fn counter_irate(pts: &[(i64, f64)]) -> Option<f64> {
    if pts.len() < 2 {
        return None;
    }
    let (t0, v0) = pts[pts.len() - 2];
    let (t1, v1) = pts[pts.len() - 1];
    let dt = (t1 - t0) as f64 / 1_000_000_000.0;
    if dt <= 0.0 {
        return None;
    }
    let dv = if v1 >= v0 { v1 - v0 } else { v1 };
    Some(dv / dt)
}

/// Reset-corrected total delta across the whole window. Each negative step
/// (counter reset) contributes the new value as fresh growth rather than a
/// negative delta.
fn counter_delta(pts: &[(i64, f64)]) -> f64 {
    let mut total = 0.0;
    for w in pts.windows(2) {
        let prev = w[0].1;
        let cur = w[1].1;
        if cur >= prev {
            total += cur - prev;
        } else {
            total += cur;
        }
    }
    total
}

/// Group key for an aggregation: the label subset that survives the grouping.
fn group_key(labels: &TsTags, grouping: &Grouping) -> TsTags {
    match grouping {
        Grouping::None => TsTags::new(),
        Grouping::By(keep) => {
            let mut out = TsTags::new();
            for k in keep {
                if let Some(v) = labels.get(k) {
                    out.insert(k.clone(), v.clone());
                }
            }
            out
        }
        Grouping::Without(drop) => {
            let mut out = labels.clone();
            for k in drop {
                out.remove(k);
            }
            out
        }
    }
}

fn eval_agg(op: AggOp, grouping: &Grouping, input: Vec<TsSample>) -> Vec<TsSample> {
    // BTreeMap keyed by the (sorted) group labels -> running accumulator. The
    // BTreeMap ordering gives a deterministic result vector.
    struct Acc {
        labels: TsTags,
        sum: f64,
        count: u64,
        min: f64,
        max: f64,
    }
    let mut groups: BTreeMap<Vec<(String, String)>, Acc> = BTreeMap::new();
    for s in input {
        let key_labels = group_key(&s.labels, grouping);
        let key: Vec<(String, String)> = key_labels
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let acc = groups.entry(key).or_insert_with(|| Acc {
            labels: key_labels.clone(),
            sum: 0.0,
            count: 0,
            min: f64::INFINITY,
            max: f64::NEG_INFINITY,
        });
        acc.sum += s.value;
        acc.count += 1;
        acc.min = acc.min.min(s.value);
        acc.max = acc.max.max(s.value);
    }
    groups
        .into_values()
        .map(|a| TsSample {
            labels: a.labels,
            value: match op {
                AggOp::Sum => a.sum,
                AggOp::Avg => a.sum / a.count as f64,
                AggOp::Min => a.min,
                AggOp::Max => a.max,
                AggOp::Count => a.count as f64,
            },
        })
        .collect()
}

/// Binary op. A scalar on either side broadcasts against every sample on the
/// other. Vector-vector ops match on the full label set (one-to-one) - samples
/// whose label sets have no partner are dropped, matching PromQL's default
/// matching behaviour for unnamed metrics.
fn eval_binary(
    op: BinOp,
    lhs: Vec<TsSample>,
    rhs: Vec<TsSample>,
) -> Result<Vec<TsSample>, TsPromQlError> {
    let l_scalar = as_scalar(&lhs);
    let r_scalar = as_scalar(&rhs);
    match (l_scalar, r_scalar) {
        (Some(lv), Some(rv)) => Ok(vec![TsSample {
            labels: TsTags::new(),
            value: apply(op, lv, rv),
        }]),
        (Some(lv), None) => Ok(rhs
            .into_iter()
            .map(|s| TsSample {
                labels: s.labels,
                value: apply(op, lv, s.value),
            })
            .collect()),
        (None, Some(rv)) => Ok(lhs
            .into_iter()
            .map(|s| TsSample {
                labels: s.labels,
                value: apply(op, s.value, rv),
            })
            .collect()),
        (None, None) => {
            let mut index: BTreeMap<Vec<(String, String)>, f64> = BTreeMap::new();
            for s in &rhs {
                index.insert(label_key(&s.labels), s.value);
            }
            let mut out = Vec::new();
            for s in lhs {
                if let Some(&rv) = index.get(&label_key(&s.labels)) {
                    out.push(TsSample {
                        labels: s.labels,
                        value: apply(op, s.value, rv),
                    });
                }
            }
            Ok(out)
        }
    }
}

/// A vector is "scalar-like" only when it is the literal single empty-label
/// sample produced by [`Expr::Scalar`]. A one-element instant vector that
/// carries labels is still a vector.
fn as_scalar(v: &[TsSample]) -> Option<f64> {
    match v {
        [s] if s.labels.is_empty() => Some(s.value),
        _ => None,
    }
}

fn label_key(labels: &TsTags) -> Vec<(String, String)> {
    labels.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
}

fn apply(op: BinOp, a: f64, b: f64) -> f64 {
    match op {
        BinOp::Add => a + b,
        BinOp::Sub => a - b,
        BinOp::Mul => a * b,
        BinOp::Div => a / b,
    }
}
