package com.submillisecond.recipes.tslazy;

import java.util.ArrayList;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;

import com.submillisecond.recipes.tsexpr.TsExpr;

/**
 * The optimiser. Three result-preserving rewrite passes over the linear
 * {@link PlanNode} list:
 *
 * <ol>
 *   <li>PREDICATE PUSHDOWN - slide each {@code Filter} up the pipeline past any
 *       {@code WithColumn} it does not read and past a {@code Select} that still
 *       carries the columns it reads.</li>
 *   <li>PROJECTION PUSHDOWN - insert an early {@code Select} so source columns
 *       no downstream node references are dropped before the scan does per-row
 *       work.</li>
 *   <li>REDUNDANT-PROJECTION ELIMINATION - collapse adjacent / no-op selects.</li>
 * </ol>
 *
 * <p>None of the passes change which rows or values {@code collect} produces.
 * Behavioural parity with the Rust sibling's {@code optimise.rs}.
 */
final class Optimiser {

    private Optimiser() {}

    static void referencedColumns(TsExpr expr, Set<String> out) {
        if (expr instanceof TsExpr.Col c) {
            out.add(c.name());
        } else if (expr instanceof TsExpr.Lit) {
            // no columns
        } else if (expr instanceof TsExpr.Unary u) {
            referencedColumns(u.operand(), out);
        } else if (expr instanceof TsExpr.Binary b) {
            referencedColumns(b.lhs(), out);
            referencedColumns(b.rhs(), out);
        } else if (expr instanceof TsExpr.Compare cmp) {
            referencedColumns(cmp.lhs(), out);
            referencedColumns(cmp.rhs(), out);
        } else if (expr instanceof TsExpr.When w) {
            referencedColumns(w.cond(), out);
            referencedColumns(w.then(), out);
            referencedColumns(w.otherwise(), out);
        } else if (expr instanceof TsExpr.Agg a) {
            referencedColumns(a.operand(), out);
        }
    }

    private static Set<String> colsOf(TsExpr expr) {
        Set<String> s = new TreeSet<>();
        referencedColumns(expr, s);
        return s;
    }

    static List<PlanNode> optimise(List<PlanNode> nodes, List<String> sourceCols) {
        List<PlanNode> work = new ArrayList<>(nodes);
        work = predicatePushdown(work);
        work = projectionPushdown(work, sourceCols);
        work = eliminateRedundantProjections(work, sourceCols);
        return work;
    }

    private static List<PlanNode> predicatePushdown(List<PlanNode> nodes) {
        List<PlanNode> out = new ArrayList<>(nodes);
        for (int i = 0; i < out.size(); i++) {
            if (out.get(i) instanceof PlanNode.Filter f) {
                Set<String> needs = colsOf(f.predicate());
                int j = i;
                while (j > 0) {
                    PlanNode prev = out.get(j - 1);
                    boolean movable;
                    if (prev instanceof PlanNode.WithColumn wc) {
                        movable = !needs.contains(wc.name());
                    } else if (prev instanceof PlanNode.Select sel) {
                        movable = sel.columns().containsAll(needs);
                    } else {
                        // Don't reorder past another filter, a sort, a limit, or
                        // an agg.
                        movable = false;
                    }
                    if (movable) {
                        PlanNode tmp = out.get(j - 1);
                        out.set(j - 1, out.get(j));
                        out.set(j, tmp);
                        j--;
                    } else {
                        break;
                    }
                }
            }
        }
        return out;
    }

    private static List<PlanNode> projectionPushdown(List<PlanNode> nodes, List<String> sourceCols) {
        if (!nodes.isEmpty() && nodes.get(0) instanceof PlanNode.Select) {
            return nodes;
        }
        Set<String> needed = new TreeSet<>();
        for (PlanNode node : nodes) {
            if (node instanceof PlanNode.Filter f) {
                referencedColumns(f.predicate(), needed);
            } else if (node instanceof PlanNode.WithColumn w) {
                referencedColumns(w.expr(), needed);
            } else if (node instanceof PlanNode.SortBy s) {
                needed.add(s.column());
            } else if (node instanceof PlanNode.Select sel) {
                needed.addAll(sel.columns());
            } else if (node instanceof PlanNode.Agg a) {
                for (PlanNode.NamedExpr ne : a.aggs()) {
                    referencedColumns(ne.expr(), needed);
                }
            }
        }
        List<String> keep = new ArrayList<>();
        for (String c : sourceCols) {
            if (needed.contains(c)) {
                keep.add(c);
            }
        }
        if (keep.isEmpty() || keep.size() == sourceCols.size()) {
            return nodes;
        }
        List<PlanNode> out = new ArrayList<>(nodes.size() + 1);
        out.add(new PlanNode.Select(keep));
        out.addAll(nodes);
        return out;
    }

    private static List<PlanNode> eliminateRedundantProjections(
            List<PlanNode> nodes, List<String> sourceCols) {
        // Fold adjacent Select pairs (the later wins).
        List<PlanNode> folded = new ArrayList<>(nodes.size());
        for (PlanNode node : nodes) {
            if (!folded.isEmpty()
                    && folded.get(folded.size() - 1) instanceof PlanNode.Select
                    && node instanceof PlanNode.Select) {
                folded.remove(folded.size() - 1);
            }
            folded.add(node);
        }

        // Drop a Select that matches the live column vector exactly.
        List<String> live = new ArrayList<>(sourceCols);
        List<PlanNode> out = new ArrayList<>(folded.size());
        for (PlanNode node : folded) {
            if (node instanceof PlanNode.Select sel) {
                if (sel.columns().equals(live)) {
                    continue;
                }
                live = new ArrayList<>(sel.columns());
                out.add(node);
            } else if (node instanceof PlanNode.WithColumn w) {
                if (!live.contains(w.name())) {
                    live.add(w.name());
                }
                out.add(node);
            } else if (node instanceof PlanNode.Agg a) {
                live = new ArrayList<>();
                for (PlanNode.NamedExpr ne : a.aggs()) {
                    live.add(ne.name());
                }
                out.add(node);
            } else {
                out.add(node);
            }
        }
        return out;
    }
}
