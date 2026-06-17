//! The optimiser. Three rewrite passes run over the linear [`PlanNode`] list,
//! all result-preserving:
//!
//! 1. PREDICATE PUSHDOWN - slide each `Filter` up the pipeline past any
//!    `WithColumn` it does not read and past a `Select` that still carries the
//!    columns it reads. Filtering earlier shrinks the row set every later node
//!    touches.
//! 2. PROJECTION PUSHDOWN - compute the set of columns any downstream node
//!    actually references and insert an early `Select` so unreferenced source
//!    columns are dropped before the scan does the per-row work.
//! 3. REDUNDANT-PROJECTION ELIMINATION - collapse a `Select` immediately
//!    followed by another `Select` (the later one wins), and drop a `Select`
//!    that is a no-op against the columns already live.
//!
//! None of the passes change which rows or values `collect` produces; the
//! `optimise_preserves_results` test pins that. The cost model that drives the
//! certificate runs on the OPTIMISED node list, so a cheaper plan certifies a
//! tighter latency budget - that is the point.

use std::collections::BTreeSet;

use subms_ts_expr::TsExpr;

use crate::plan::PlanNode;

/// Collect the set of column names a [`TsExpr`] references (its `Col` leaves).
pub(crate) fn referenced_columns(expr: &TsExpr, out: &mut BTreeSet<String>) {
    match expr {
        TsExpr::Col(name) => {
            out.insert(name.clone());
        }
        TsExpr::Lit(_) => {}
        TsExpr::Unary(_, operand) => referenced_columns(operand, out),
        TsExpr::Binary(_, lhs, rhs) | TsExpr::Compare(_, lhs, rhs) => {
            referenced_columns(lhs, out);
            referenced_columns(rhs, out);
        }
        TsExpr::When {
            cond,
            then,
            otherwise,
        } => {
            referenced_columns(cond, out);
            referenced_columns(then, out);
            referenced_columns(otherwise, out);
        }
        TsExpr::Agg(_, operand) => referenced_columns(operand, out),
    }
}

fn cols_of(expr: &TsExpr) -> BTreeSet<String> {
    let mut s = BTreeSet::new();
    referenced_columns(expr, &mut s);
    s
}

/// Run the three passes to a fixpoint-ish single sweep each. The order matters:
/// predicate pushdown first (so filters sit early), then projection pushdown
/// (which reads the now-final reference set), then redundant-projection cleanup.
pub(crate) fn optimise_nodes(nodes: &[PlanNode], source_cols: &[String]) -> Vec<PlanNode> {
    let mut work = nodes.to_vec();
    work = predicate_pushdown(work);
    work = projection_pushdown(work, source_cols);
    work = eliminate_redundant_projections(work, source_cols);
    work
}

// Slide each Filter as early as legal. A Filter may move above an earlier
// WithColumn only if it does not read that column; it may move above a Select
// only if the Select still projects every column the predicate reads (else the
// predicate would reference a dropped column). It never moves above another
// Filter (order among filters is irrelevant to the result, so leave it). An Agg
// is terminal and nothing moves past it.
fn predicate_pushdown(nodes: Vec<PlanNode>) -> Vec<PlanNode> {
    let mut out = nodes;
    let mut i = 0;
    while i < out.len() {
        if let PlanNode::Filter(pred) = &out[i] {
            let needs = cols_of(pred);
            let mut j = i;
            while j > 0 {
                let movable = match &out[j - 1] {
                    PlanNode::WithColumn(name, _) => !needs.contains(name),
                    PlanNode::Select(cols) => needs.iter().all(|c| cols.iter().any(|p| p == c)),
                    // Don't reorder past another filter, a sort, a limit (a
                    // filter-before-limit changes which rows survive), or an agg.
                    _ => false,
                };
                if movable {
                    out.swap(j - 1, j);
                    j -= 1;
                } else {
                    break;
                }
            }
        }
        i += 1;
    }
    out
}

// Insert an early Select projecting exactly the columns any downstream node
// references, when that is a strict subset of the source columns. Dropping
// unreferenced columns up front shrinks the aligned-view materialisation every
// later node pays for. Skipped when the first node is already a Select (that
// Select is handled by the elimination pass) or when nothing is dropped.
fn projection_pushdown(nodes: Vec<PlanNode>, source_cols: &[String]) -> Vec<PlanNode> {
    if matches!(nodes.first(), Some(PlanNode::Select(_))) {
        return nodes;
    }
    let mut needed: BTreeSet<String> = BTreeSet::new();
    for node in &nodes {
        match node {
            PlanNode::Filter(e) => referenced_columns(e, &mut needed),
            PlanNode::WithColumn(_, e) => referenced_columns(e, &mut needed),
            PlanNode::SortBy { column, .. } => {
                needed.insert(column.clone());
            }
            PlanNode::Select(cols) => {
                for c in cols {
                    needed.insert(c.clone());
                }
            }
            PlanNode::Agg(aggs) => {
                for (_, e) in aggs {
                    referenced_columns(e, &mut needed);
                }
            }
            PlanNode::Limit(_) => {}
        }
    }
    // Only keep source columns that are actually referenced, preserving source
    // order. A WithColumn output referenced later is not a source column, so it
    // simply does not appear here - the early Select stays a subset of source.
    let keep: Vec<String> = source_cols
        .iter()
        .filter(|c| needed.contains(*c))
        .cloned()
        .collect();
    if keep.is_empty() || keep.len() == source_cols.len() {
        return nodes;
    }
    let mut out = Vec::with_capacity(nodes.len() + 1);
    out.push(PlanNode::Select(keep));
    out.extend(nodes);
    out
}

// Collapse adjacent Selects (the later wins) and drop a Select that projects
// exactly the live columns in the same order (a no-op). `live` tracks the
// column set entering each node.
fn eliminate_redundant_projections(nodes: Vec<PlanNode>, source_cols: &[String]) -> Vec<PlanNode> {
    // First fold adjacent Select pairs.
    let mut folded: Vec<PlanNode> = Vec::with_capacity(nodes.len());
    for node in nodes {
        if let (Some(PlanNode::Select(_)), PlanNode::Select(_)) = (folded.last(), &node) {
            folded.pop();
        }
        folded.push(node);
    }

    // Then drop a Select that matches the live column vector exactly.
    let mut live: Vec<String> = source_cols.to_vec();
    let mut out: Vec<PlanNode> = Vec::with_capacity(folded.len());
    for node in folded {
        match &node {
            PlanNode::Select(cols) => {
                if cols == &live {
                    continue;
                }
                live = cols.clone();
                out.push(node);
            }
            PlanNode::WithColumn(name, _) => {
                if !live.iter().any(|c| c == name) {
                    live.push(name.clone());
                }
                out.push(node);
            }
            PlanNode::Agg(aggs) => {
                live = aggs.iter().map(|(n, _)| n.clone()).collect();
                out.push(node);
            }
            _ => out.push(node),
        }
    }
    out
}
