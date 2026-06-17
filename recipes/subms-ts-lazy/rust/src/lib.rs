//! `subms-ts-lazy` - a deferred query planner over the typed
//! [`TsDataFrame`](subms_ts::TsDataFrame). A [`LazyTsFrame`] records a linear
//! pipeline of operations WITHOUT executing, rewrites it with result-preserving
//! optimiser passes, and lowers the plan to a `subms-ts-plan`
//! [`TsLatencyCertificate`](subms_ts_plan::TsLatencyCertificate) - a signed
//! per-query latency guarantee.
//!
//! The certificate is the headline. Every recipe in the cookbook arc publishes
//! a per-operation p99 contract. On their own those are point facts about one
//! op. A lazy query is a sequence of ops, so its system p99 is the sum of the
//! per-op costs plus a planner overhead. [`certify`](LazyTsFrame::certify)
//! walks the OPTIMISED plan, assigns each node a representative per-op p99 from
//! a small cost model, and composes them into one certificate an SRE can put in
//! an SLA. Optimisation makes the plan cheaper, so a tighter plan certifies a
//! tighter budget - this is the analytical engine you can put in a latency SLA.
//!
//! Scope is deliberately bounded to a LINEAR pipeline: `select`, `filter`,
//! `with_column`, `sort_by`, `limit`, and a terminal `agg`. Every one is
//! expressible via `subms-ts-expr` eval, so this recipe depends on `subms-ts` +
//! `subms-ts-expr` + `subms-ts-plan` and nothing else in the operator arc.
//! Group-by and join are the eager standalone operators a caller composes
//! AROUND a lazy pipeline (or future plan nodes); lazy v0 does not own them.
//!
//! ```
//! use subms_ts::{TsColumn, TsDataFrame, TsSeries};
//! use subms_ts_expr::TsExpr;
//! use subms_ts_lazy::LazyTsFrame;
//!
//! let mut px = TsSeries::<f64>::new();
//! let mut qty = TsSeries::<i64>::new();
//! for i in 0..8 {
//!     px.push(i, i as f64).unwrap();
//!     qty.push(i, i).unwrap();
//! }
//! let frame = TsDataFrame::new()
//!     .with_column("px", TsColumn::F64(px))
//!     .with_column("qty", TsColumn::I64(qty));
//!
//! let cert = LazyTsFrame::new(frame)
//!     .filter(TsExpr::col("px").gt(TsExpr::lit_f64(2.0)))
//!     .with_column("notional", TsExpr::col("px").mul(TsExpr::col("qty")))
//!     .select(&["notional"])
//!     .certify("ci-dedicated", 0);
//!
//! assert!(cert.meets_budget(1_000_000)); // composes to a sub-ms certify budget
//! assert!(cert.verify());
//! ```

mod exec;
mod optimise;
mod plan;

pub use exec::{LazyError, ResultFrame};
pub use plan::{LazyTsFrame, PlanNode};

use subms_ts_expr::TsExpr;
use subms_ts_plan::{TsLatencyCertificate, TsPlan};

/// Flat planner overhead added on top of the per-node costs, in nanoseconds.
/// Covers IR walk + plan assembly + certificate hash - work the planner does
/// once per query, independent of row count. A model figure, not a measurement.
pub const PLANNER_OVERHEAD_NS: u64 = 40_000;

/// Per-op representative p99 costs (nanoseconds) for a frame on the recipe's
/// canonical row scale. These are a COST MODEL, not a per-deployment
/// measurement: they encode the shape (a scan op is O(rows); an `agg` reduce
/// folds the rows; a `sort_by` is O(rows log rows)) at representative figures
/// taken from the analytical-front bench. A real deployment recalibrates them
/// against its own captured perf JSON before putting the certificate in an SLA.
mod cost {
    pub const SELECT_NS: u64 = 8_000; // pointer reshuffle, near-free
    pub const FILTER_NS: u64 = 90_000; // eval predicate + gather passing rows
    pub const WITH_COLUMN_NS: u64 = 110_000; // eval expr + append a column
    pub const SORT_NS: u64 = 160_000; // key extract + stable sort permutation
    pub const LIMIT_NS: u64 = 6_000; // truncate, near-free
    pub const AGG_NS: u64 = 70_000; // one reduce pass per output aggregate
}

impl LazyTsFrame {
    /// Apply the result-preserving optimiser passes (predicate pushdown,
    /// projection pushdown, redundant-projection elimination), returning a new
    /// lazy frame whose plan is the rewritten node list. Idempotent: optimising
    /// an already-optimised plan is a no-op.
    pub fn optimise(self) -> LazyTsFrame {
        let source_cols: Vec<String> = self.source.column_names().map(|s| s.to_string()).collect();
        let nodes = optimise::optimise_nodes(self.nodes(), &source_cols);
        LazyTsFrame {
            source: self.source,
            nodes,
        }
    }

    /// Execute the OPTIMISED plan, returning a [`ResultFrame`] (a ts axis plus
    /// named typed columns, in pipeline row order). Use
    /// [`ResultFrame::into_data_frame`] for a [`TsDataFrame`] view.
    pub fn collect(self) -> Result<ResultFrame, LazyError> {
        let source_cols: Vec<String> = self.source.column_names().map(|s| s.to_string()).collect();
        let nodes = optimise::optimise_nodes(&self.nodes, &source_cols);
        exec::run_plan(&self.source, &nodes)
    }

    /// Execute WITHOUT optimising - the raw recorded plan. Used by the
    /// optimise-preserves-results test as the reference, and when a caller
    /// wants to bypass the rewrite.
    pub fn collect_unoptimised(self) -> Result<ResultFrame, LazyError> {
        exec::run_plan(&self.source, &self.nodes)
    }

    /// Terminal whole-frame aggregation: each `(name, expr)` is reduced to a
    /// scalar via `eval_scalar`, emitting a one-row [`ResultFrame`]. The plan is
    /// optimised before the reduce.
    pub fn agg(self, aggs: &[(&str, TsExpr)]) -> Result<ResultFrame, LazyError> {
        let owned: Vec<(String, TsExpr)> = aggs
            .iter()
            .map(|(n, e)| (n.to_string(), e.clone()))
            .collect();
        self.push_agg(owned).collect()
    }

    /// Render the OPTIMISED plan as a multi-line EXPLAIN string, one node per
    /// line in execution order. This is the surface a reader inspects to confirm
    /// a pushdown fired.
    pub fn explain(&self) -> String {
        let source_cols: Vec<String> = self.source.column_names().map(|s| s.to_string()).collect();
        let nodes = optimise::optimise_nodes(&self.nodes, &source_cols);
        explain_nodes(&nodes)
    }

    /// Render the UN-optimised (recorded) plan. Pairs with [`explain`](Self::explain)
    /// so a test can assert a pass reordered or dropped a node.
    pub fn explain_unoptimised(&self) -> String {
        explain_nodes(&self.nodes)
    }

    /// THE MOAT. Lower the OPTIMISED plan to a [`TsLatencyCertificate`]: append
    /// a [`TsPlan`] stage per node citing the cost model's representative p99,
    /// add the planner overhead, and certify for `hardware_tier` valid until
    /// `valid_until` (epoch-nanos, 0 = unbounded). The certificate's
    /// `total_p99_ns` is the sum of the per-node costs plus the overhead, and
    /// its `verify()` holds over the canonical body.
    pub fn certify(
        &self,
        hardware_tier: impl Into<String>,
        valid_until: i64,
    ) -> TsLatencyCertificate {
        self.build_plan().certify(hardware_tier, valid_until)
    }

    /// The [`TsPlan`] the certificate is built from, exposed so a caller can
    /// inspect the per-stage breakdown or re-certify for a different tier.
    pub fn build_plan(&self) -> TsPlan {
        let source_cols: Vec<String> = self.source.column_names().map(|s| s.to_string()).collect();
        let nodes = optimise::optimise_nodes(&self.nodes, &source_cols);
        let mut plan = TsPlan::new().with_overhead(PLANNER_OVERHEAD_NS);
        for node in &nodes {
            plan = plan.then("subms-ts-lazy", node.kind(), node_cost_ns(node));
        }
        plan
    }
}

/// The cost-model p99 (ns) for a single plan node. Public so the writeup and
/// tests can cite the exact figures; this is a representative model, not a
/// per-deployment measurement.
pub fn node_cost_ns(node: &PlanNode) -> u64 {
    match node {
        PlanNode::Select(_) => cost::SELECT_NS,
        PlanNode::Filter(_) => cost::FILTER_NS,
        PlanNode::WithColumn(_, _) => cost::WITH_COLUMN_NS,
        PlanNode::SortBy { .. } => cost::SORT_NS,
        PlanNode::Limit(_) => cost::LIMIT_NS,
        // One reduce pass per output aggregate.
        PlanNode::Agg(aggs) => cost::AGG_NS.saturating_mul(aggs.len().max(1) as u64),
    }
}

fn explain_nodes(nodes: &[PlanNode]) -> String {
    if nodes.is_empty() {
        return "LazyTsFrame (identity: source scan)".to_string();
    }
    let mut out = String::from("LazyTsFrame plan:");
    for (i, node) in nodes.iter().enumerate() {
        out.push_str(&format!("\n  {i}: {}", describe_node(node)));
    }
    out
}

fn describe_node(node: &PlanNode) -> String {
    match node {
        PlanNode::Select(cols) => format!("Select [{}]", cols.join(", ")),
        PlanNode::Filter(_) => "Filter <predicate>".to_string(),
        PlanNode::WithColumn(name, _) => format!("WithColumn {name} = <expr>"),
        PlanNode::SortBy { column, ascending } => {
            let dir = if *ascending { "asc" } else { "desc" };
            format!("SortBy {column} {dir}")
        }
        PlanNode::Limit(n) => format!("Limit {n}"),
        PlanNode::Agg(aggs) => {
            let names: Vec<&str> = aggs.iter().map(|(n, _)| n.as_str()).collect();
            format!("Agg [{}]", names.join(", "))
        }
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
