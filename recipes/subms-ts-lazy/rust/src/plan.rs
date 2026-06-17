//! The plan IR. A [`LazyTsFrame`] records a linear pipeline of [`PlanNode`]s
//! over a source [`TsDataFrame`] WITHOUT executing - building, optimising, and
//! certifying all walk this node list; only [`collect`](LazyTsFrame::collect)
//! touches data.
//!
//! The pipeline is deliberately linear (no joins / group-by nodes): every node
//! is expressible through `subms-ts-expr` eval over the whole frame, so the
//! planner depends on `subms-ts` + `subms-ts-expr` + `subms-ts-plan` and
//! nothing else in the operator arc. Group-by and join are the eager standalone
//! operators a caller composes AROUND a lazy pipeline.

use subms_ts::TsDataFrame;
use subms_ts_expr::TsExpr;

/// One step of a lazy pipeline. `Agg` is terminal (whole-frame reduction to a
/// one-row result); the rest are row-preserving or row-shaping transforms that
/// can chain freely. The variants are public so the optimiser can match and
/// rewrite them.
#[derive(Clone, Debug, PartialEq)]
pub enum PlanNode {
    /// Keep only the named columns, in the requested order.
    Select(Vec<String>),
    /// Keep rows where `predicate` evaluates to a `true` Bool cell.
    Filter(TsExpr),
    /// Append a derived column `name` computed from `expr`.
    WithColumn(String, TsExpr),
    /// Reorder rows by `column`, ascending or descending.
    SortBy { column: String, ascending: bool },
    /// Keep the first `n` rows.
    Limit(usize),
    /// Terminal: reduce each `(name, expr)` to a scalar, emitting a one-row
    /// frame. An `Agg` node only ever appears last.
    Agg(Vec<(String, TsExpr)>),
}

impl PlanNode {
    /// A short stable kind tag, used by `explain` and the cost model. Distinct
    /// from `Debug` so the certificate's stage names stay stable under IR edits.
    pub fn kind(&self) -> &'static str {
        match self {
            PlanNode::Select(_) => "select",
            PlanNode::Filter(_) => "filter",
            PlanNode::WithColumn(_, _) => "with_column",
            PlanNode::SortBy { .. } => "sort_by",
            PlanNode::Limit(_) => "limit",
            PlanNode::Agg(_) => "agg",
        }
    }
}

/// A deferred query over a source frame. Each builder method appends a
/// [`PlanNode`] and returns `self`; nothing runs until a terminal
/// ([`collect`](Self::collect), [`agg`](Self::agg)) or
/// [`certify`](Self::certify) is called. Cheap to clone the plan (it clones the
/// node list, not the source data).
pub struct LazyTsFrame {
    pub(crate) source: TsDataFrame,
    pub(crate) nodes: Vec<PlanNode>,
}

impl LazyTsFrame {
    /// Wrap a source frame in an empty (identity) plan.
    pub fn new(source: TsDataFrame) -> Self {
        Self {
            source,
            nodes: Vec::new(),
        }
    }

    /// Project to the named columns, in order. Unknown names are dropped at
    /// execution time (matching `TsDataFrame::select`).
    pub fn select(mut self, columns: &[&str]) -> Self {
        self.nodes.push(PlanNode::Select(
            columns.iter().map(|s| s.to_string()).collect(),
        ));
        self
    }

    /// Keep rows where `predicate` is a `true` Bool cell. A null (invalid) cell
    /// drops the row - a missing predicate is not a pass.
    pub fn filter(mut self, predicate: TsExpr) -> Self {
        self.nodes.push(PlanNode::Filter(predicate));
        self
    }

    /// Append a derived column. If `name` already exists it is replaced in
    /// place (re-deriving a column is not a duplicate-column error).
    pub fn with_column(mut self, name: impl Into<String>, expr: TsExpr) -> Self {
        self.nodes.push(PlanNode::WithColumn(name.into(), expr));
        self
    }

    /// Reorder rows by `column`. A row whose sort key is null sorts last in
    /// both directions (nulls-last), matching the analytical default.
    pub fn sort_by(mut self, column: impl Into<String>, ascending: bool) -> Self {
        self.nodes.push(PlanNode::SortBy {
            column: column.into(),
            ascending,
        });
        self
    }

    /// Truncate to the first `n` rows.
    pub fn limit(mut self, n: usize) -> Self {
        self.nodes.push(PlanNode::Limit(n));
        self
    }

    /// The plan nodes as recorded (pre-optimise). For inspection and tests.
    pub fn nodes(&self) -> &[PlanNode] {
        &self.nodes
    }

    pub(crate) fn push_agg(mut self, aggs: Vec<(String, TsExpr)>) -> Self {
        self.nodes.push(PlanNode::Agg(aggs));
        self
    }
}
