package com.submillisecond.recipes.tslazy;

import java.util.ArrayList;
import java.util.List;

import com.submillisecond.recipes.ts.TsDataFrame;
import com.submillisecond.recipes.tsexpr.TsExpr;
import com.submillisecond.recipes.tsplan.TsLatencyCertificate;
import com.submillisecond.recipes.tsplan.TsPlan;

/**
 * A deferred query planner over the typed {@link TsDataFrame}. A
 * {@code LazyTsFrame} records a linear pipeline of {@link PlanNode}s WITHOUT
 * executing, rewrites it with result-preserving optimiser passes, and lowers the
 * plan to a {@code subms-ts-plan} {@link TsLatencyCertificate} - a signed
 * per-query latency guarantee.
 *
 * <p>The certificate is the headline. Each cookbook recipe publishes a per-op
 * p99 contract; a lazy query is a sequence of ops, so its system p99 is the sum
 * of the per-op costs plus a planner overhead. {@link #certify} walks the
 * OPTIMISED plan, assigns each node a representative per-op p99 from a small cost
 * model, and composes them into one certificate an SRE can put in an SLA.
 *
 * <p>Scope is a LINEAR pipeline ({@code select}, {@code filter},
 * {@code withColumn}, {@code sortBy}, {@code limit}, terminal {@code agg}); each
 * is expressible via {@code subms-ts-expr} eval, so this recipe depends on
 * {@code subms-ts} + {@code subms-ts-expr} + {@code subms-ts-plan} and nothing
 * else in the operator arc. Group-by and join are the eager standalone operators
 * a caller composes AROUND a lazy pipeline.
 *
 * <p>Byte-equivalent in shape and certificate output to the Rust sibling.
 */
public final class LazyTsFrame {

    /**
     * Flat planner overhead added on top of the per-node costs, in nanoseconds.
     * Covers IR walk + plan assembly + certificate hash. A model figure, not a
     * measurement.
     */
    public static final long PLANNER_OVERHEAD_NS = 40_000L;

    // Per-op representative p99 costs (ns) - a COST MODEL, not a per-deployment
    // measurement. They encode the op shape at representative figures from the
    // analytical-front bench. A deployment recalibrates against its own perf
    // JSON before putting the certificate in an SLA. Mirrors the Rust `cost`.
    private static final long SELECT_NS = 8_000L;
    private static final long FILTER_NS = 90_000L;
    private static final long WITH_COLUMN_NS = 110_000L;
    private static final long SORT_NS = 160_000L;
    private static final long LIMIT_NS = 6_000L;
    private static final long AGG_NS = 70_000L;

    private final TsDataFrame source;
    private final List<PlanNode> nodes;

    /** Wrap a source frame in an empty (identity) plan. */
    public LazyTsFrame(TsDataFrame source) {
        this(source, new ArrayList<>());
    }

    private LazyTsFrame(TsDataFrame source, List<PlanNode> nodes) {
        this.source = source;
        this.nodes = nodes;
    }

    private LazyTsFrame add(PlanNode node) {
        List<PlanNode> next = new ArrayList<>(nodes);
        next.add(node);
        return new LazyTsFrame(source, next);
    }

    /** Project to the named columns, in order. Unknown names drop at exec time. */
    public LazyTsFrame select(String... columns) {
        return add(new PlanNode.Select(List.of(columns)));
    }

    /** Keep rows where {@code predicate} is a true Bool cell (null drops). */
    public LazyTsFrame filter(TsExpr predicate) {
        return add(new PlanNode.Filter(predicate));
    }

    /** Append (or replace) a derived column computed from {@code expr}. */
    public LazyTsFrame withColumn(String name, TsExpr expr) {
        return add(new PlanNode.WithColumn(name, expr));
    }

    /** Reorder rows by {@code column}. Null keys sort last in both directions. */
    public LazyTsFrame sortBy(String column, boolean ascending) {
        return add(new PlanNode.SortBy(column, ascending));
    }

    /** Truncate to the first {@code n} rows. */
    public LazyTsFrame limit(int n) {
        return add(new PlanNode.Limit(n));
    }

    /** The plan nodes as recorded (pre-optimise). */
    public List<PlanNode> nodes() {
        return List.copyOf(nodes);
    }

    private List<String> sourceCols() {
        return new ArrayList<>(source.columnNames());
    }

    /** Apply the result-preserving optimiser passes, returning a new lazy frame. */
    public LazyTsFrame optimise() {
        return new LazyTsFrame(source, Optimiser.optimise(nodes, sourceCols()));
    }

    /** Execute the OPTIMISED plan, returning a {@link ResultFrame}. */
    public ResultFrame collect() {
        return Executor.runPlan(source, Optimiser.optimise(nodes, sourceCols()));
    }

    /** Execute WITHOUT optimising - the raw recorded plan. */
    public ResultFrame collectUnoptimised() {
        return Executor.runPlan(source, nodes);
    }

    /**
     * Terminal whole-frame aggregation: each {@code (name, expr)} is reduced to
     * a scalar, emitting a one-row {@link ResultFrame}. The plan is optimised
     * before the reduce.
     */
    public ResultFrame agg(List<PlanNode.NamedExpr> aggs) {
        return add(new PlanNode.Agg(aggs)).collect();
    }

    /** Render the OPTIMISED plan as a multi-line EXPLAIN string. */
    public String explain() {
        return explainNodes(Optimiser.optimise(nodes, sourceCols()));
    }

    /** Render the UN-optimised (recorded) plan. */
    public String explainUnoptimised() {
        return explainNodes(nodes);
    }

    /**
     * THE MOAT. Lower the OPTIMISED plan to a {@link TsLatencyCertificate}:
     * append a {@link TsPlan} stage per node citing the cost model's
     * representative p99, add the planner overhead, and certify for
     * {@code hardwareTier} valid until {@code validUntil} (epoch-nanos, 0 =
     * unbounded). The certificate's {@code totalP99Ns} is the sum of the
     * per-node costs plus the overhead.
     */
    public TsLatencyCertificate certify(String hardwareTier, long validUntil) {
        return buildPlan().certify(hardwareTier, validUntil);
    }

    /** The {@link TsPlan} the certificate is built from. */
    public TsPlan buildPlan() {
        List<PlanNode> opt = Optimiser.optimise(nodes, sourceCols());
        TsPlan plan = new TsPlan().withOverhead(PLANNER_OVERHEAD_NS);
        for (PlanNode node : opt) {
            plan = plan.then("subms-ts-lazy", node.kind(), nodeCostNs(node));
        }
        return plan;
    }

    /**
     * The cost-model p99 (ns) for a single plan node. A representative model,
     * not a per-deployment measurement.
     */
    public static long nodeCostNs(PlanNode node) {
        if (node instanceof PlanNode.Select) {
            return SELECT_NS;
        } else if (node instanceof PlanNode.Filter) {
            return FILTER_NS;
        } else if (node instanceof PlanNode.WithColumn) {
            return WITH_COLUMN_NS;
        } else if (node instanceof PlanNode.SortBy) {
            return SORT_NS;
        } else if (node instanceof PlanNode.Limit) {
            return LIMIT_NS;
        } else {
            PlanNode.Agg a = (PlanNode.Agg) node;
            return AGG_NS * Math.max(1, a.aggs().size());
        }
    }

    private static String explainNodes(List<PlanNode> nodes) {
        if (nodes.isEmpty()) {
            return "LazyTsFrame (identity: source scan)";
        }
        StringBuilder out = new StringBuilder("LazyTsFrame plan:");
        for (int i = 0; i < nodes.size(); i++) {
            out.append("\n  ").append(i).append(": ").append(describeNode(nodes.get(i)));
        }
        return out.toString();
    }

    private static String describeNode(PlanNode node) {
        if (node instanceof PlanNode.Select s) {
            return "Select [" + String.join(", ", s.columns()) + "]";
        } else if (node instanceof PlanNode.Filter) {
            return "Filter <predicate>";
        } else if (node instanceof PlanNode.WithColumn w) {
            return "WithColumn " + w.name() + " = <expr>";
        } else if (node instanceof PlanNode.SortBy sb) {
            return "SortBy " + sb.column() + " " + (sb.ascending() ? "asc" : "desc");
        } else if (node instanceof PlanNode.Limit l) {
            return "Limit " + l.n();
        } else {
            PlanNode.Agg a = (PlanNode.Agg) node;
            List<String> names = new ArrayList<>(a.aggs().size());
            for (PlanNode.NamedExpr ne : a.aggs()) {
                names.add(ne.name());
            }
            return "Agg [" + String.join(", ", names) + "]";
        }
    }
}
