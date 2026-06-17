package com.submillisecond.recipes.tspromql;

import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;

import com.submillisecond.recipes.ts.TsCollection;
import com.submillisecond.recipes.ts.TsPoint;
import com.submillisecond.recipes.ts.TsSeries;
import com.submillisecond.recipes.ts.TsSeriesMetadata;
import com.submillisecond.recipes.tspromql.Ast.Agg;
import com.submillisecond.recipes.tspromql.Ast.Binary;
import com.submillisecond.recipes.tspromql.Ast.Expr;
import com.submillisecond.recipes.tspromql.Ast.Func;
import com.submillisecond.recipes.tspromql.Ast.FuncKind;
import com.submillisecond.recipes.tspromql.Ast.GroupKind;
import com.submillisecond.recipes.tspromql.Ast.Grouping;
import com.submillisecond.recipes.tspromql.Ast.Scalar;
import com.submillisecond.recipes.tspromql.Ast.Selector;
import com.submillisecond.recipes.tspromql.Ast.SelectorExpr;

/**
 * Tree-walking evaluator: walk a parsed {@link Expr} against a
 * {@code TsCollection<Double>} at an instant, resolving selectors to the set of
 * series whose {@code __name__} (or metadata name) + label tags match. Mirrors
 * the Rust {@code eval} module.
 */
final class PromQlEval {

    /** Reserved metric-name label, mirroring Prometheus' {@code __name__}. */
    static final String METRIC_LABEL = "__name__";

    private final TsCollection<Double> coll;

    PromQlEval(TsCollection<Double> coll) {
        this.coll = coll;
    }

    TsPromQlResult evalInstant(Expr expr, long at) {
        return new TsPromQlResult(eval(expr, at));
    }

    private List<TsSample> eval(Expr expr, long at) {
        return switch (expr) {
            case Scalar s -> List.of(new TsSample(Map.of(), s.value()));
            case SelectorExpr se -> evalSelectorInstant(se.selector(), at);
            case Func f -> evalFunc(f.kind(), f.selector(), f.rangeNs(), at);
            case Agg a -> evalAgg(a, eval(a.inner(), at));
            case Binary b -> evalBinary(b, eval(b.lhs(), at), eval(b.rhs(), at));
        };
    }

    // Series matched by a selector, sorted by id for a deterministic vector.
    private List<TsSeries<Double>> matchedSeries(Selector sel) {
        List<TsSeries<Double>> matched = new ArrayList<>();
        List<Long> ids = new ArrayList<>();
        for (TsSeries<Double> s : coll.series()) {
            Optional<TsSeriesMetadata> mo = s.metadata();
            if (mo.isEmpty()) {
                continue;
            }
            TsSeriesMetadata m = mo.get();
            String name = m.tags().getOrDefault(METRIC_LABEL, m.name());
            if (!name.equals(sel.metric())) {
                continue;
            }
            boolean ok = true;
            for (Ast.LabelMatcher matcher : sel.matchers()) {
                if (!matcher.matches(m.tags().get(matcher.label()))) {
                    ok = false;
                    break;
                }
            }
            if (ok) {
                matched.add(s);
                ids.add(m.id());
            }
        }
        // sort matched by the parallel id list
        List<Integer> order = new ArrayList<>();
        for (int i = 0; i < matched.size(); i++) {
            order.add(i);
        }
        order.sort((a, b) -> Long.compare(ids.get(a), ids.get(b)));
        List<TsSeries<Double>> sorted = new ArrayList<>(matched.size());
        for (int idx : order) {
            sorted.add(matched.get(idx));
        }
        return sorted;
    }

    private List<TsSample> evalSelectorInstant(Selector sel, long at) {
        long probe = at - sel.offsetNs();
        List<TsSample> out = new ArrayList<>();
        for (TsSeries<Double> s : matchedSeries(sel)) {
            Optional<TsPoint<Double>> p = s.nearestBefore(probe);
            if (p.isPresent()) {
                out.add(new TsSample(outputLabels(s), p.get().value()));
            }
        }
        return out;
    }

    // Series labels with the reserved __name__ stripped.
    private static Map<String, String> outputLabels(TsSeries<Double> s) {
        TreeMap<String, String> t = new TreeMap<>();
        s.metadata().ifPresent(m -> t.putAll(m.tags()));
        t.remove(METRIC_LABEL);
        return t;
    }

    private List<TsSample> evalFunc(FuncKind kind, Selector sel, long rangeNs, long at) {
        long hi = at - sel.offsetNs();
        long lo = hi - rangeNs;
        List<TsSample> out = new ArrayList<>();
        for (TsSeries<Double> s : matchedSeries(sel)) {
            List<Double> vals = new ArrayList<>();
            List<Long> times = new ArrayList<>();
            for (TsPoint<Double> p : s.range(lo, hi)) {
                times.add(p.ts());
                vals.add(p.value());
            }
            Double value = switch (kind) {
                case RATE -> counterRate(times, vals);
                case INCREASE -> counterIncrease(times, vals);
                case IRATE -> counterIrate(times, vals);
            };
            if (value != null) {
                out.add(new TsSample(outputLabels(s), value));
            }
        }
        return out;
    }

    private static Double counterRate(List<Long> times, List<Double> vals) {
        if (vals.size() < 2) {
            return null;
        }
        long spanNs = times.get(times.size() - 1) - times.get(0);
        if (spanNs <= 0) {
            return null;
        }
        double delta = counterDelta(vals);
        return delta / (spanNs / 1_000_000_000.0);
    }

    private static Double counterIncrease(List<Long> times, List<Double> vals) {
        if (vals.size() < 2) {
            return null;
        }
        return counterDelta(vals);
    }

    private static Double counterIrate(List<Long> times, List<Double> vals) {
        int n = vals.size();
        if (n < 2) {
            return null;
        }
        double dt = (times.get(n - 1) - times.get(n - 2)) / 1_000_000_000.0;
        if (dt <= 0.0) {
            return null;
        }
        double v0 = vals.get(n - 2);
        double v1 = vals.get(n - 1);
        double dv = v1 >= v0 ? v1 - v0 : v1;
        return dv / dt;
    }

    // Reset-corrected total delta: a negative step counts the new value as
    // fresh growth (the counter reset to zero before climbing again).
    private static double counterDelta(List<Double> vals) {
        double total = 0.0;
        for (int i = 1; i < vals.size(); i++) {
            double prev = vals.get(i - 1);
            double cur = vals.get(i);
            total += cur >= prev ? cur - prev : cur;
        }
        return total;
    }

    private static Map<String, String> groupKey(Map<String, String> labels, Grouping g) {
        TreeMap<String, String> out = new TreeMap<>();
        switch (g.kind()) {
            case NONE -> { }
            case BY -> {
                for (String k : g.labels()) {
                    String v = labels.get(k);
                    if (v != null) {
                        out.put(k, v);
                    }
                }
            }
            case WITHOUT -> {
                out.putAll(labels);
                for (String k : g.labels()) {
                    out.remove(k);
                }
            }
        }
        return out;
    }

    private static final class Acc {
        final TreeMap<String, String> labels;
        double sum = 0.0;
        long count = 0;
        double min = Double.POSITIVE_INFINITY;
        double max = Double.NEGATIVE_INFINITY;

        Acc(Map<String, String> labels) {
            this.labels = new TreeMap<>(labels);
        }
    }

    private List<TsSample> evalAgg(Agg agg, List<TsSample> input) {
        TreeMap<TreeMap<String, String>, Acc> groups = new TreeMap<>(PromQlEval::compareLabels);
        for (TsSample s : input) {
            TreeMap<String, String> key = new TreeMap<>(groupKey(s.labels(), agg.grouping()));
            Acc acc = groups.computeIfAbsent(key, Acc::new);
            acc.sum += s.value();
            acc.count++;
            acc.min = Math.min(acc.min, s.value());
            acc.max = Math.max(acc.max, s.value());
        }
        List<TsSample> out = new ArrayList<>(groups.size());
        for (Acc a : groups.values()) {
            double v = switch (agg.op()) {
                case SUM -> a.sum;
                case AVG -> a.sum / a.count;
                case MIN -> a.min;
                case MAX -> a.max;
                case COUNT -> (double) a.count;
            };
            out.add(new TsSample(a.labels, v));
        }
        return out;
    }

    private List<TsSample> evalBinary(Binary b, List<TsSample> lhs, List<TsSample> rhs) {
        Double l = asScalar(lhs);
        Double r = asScalar(rhs);
        List<TsSample> out = new ArrayList<>();
        if (l != null && r != null) {
            out.add(new TsSample(Map.of(), apply(b.op(), l, r)));
        } else if (l != null) {
            for (TsSample s : rhs) {
                out.add(new TsSample(s.labels(), apply(b.op(), l, s.value())));
            }
        } else if (r != null) {
            for (TsSample s : lhs) {
                out.add(new TsSample(s.labels(), apply(b.op(), s.value(), r)));
            }
        } else {
            TreeMap<TreeMap<String, String>, Double> index = new TreeMap<>(PromQlEval::compareLabels);
            for (TsSample s : rhs) {
                index.put(new TreeMap<>(s.labels()), s.value());
            }
            for (TsSample s : lhs) {
                Double rv = index.get(new TreeMap<>(s.labels()));
                if (rv != null) {
                    out.add(new TsSample(s.labels(), apply(b.op(), s.value(), rv)));
                }
            }
        }
        return out;
    }

    // A vector is scalar-like only when it is the single empty-label sample
    // produced by a literal; a labelled one-element vector stays a vector.
    private static Double asScalar(List<TsSample> v) {
        if (v.size() == 1 && v.get(0).labels().isEmpty()) {
            return v.get(0).value();
        }
        return null;
    }

    private static double apply(Ast.BinOp op, double a, double b) {
        return switch (op) {
            case ADD -> a + b;
            case SUB -> a - b;
            case MUL -> a * b;
            case DIV -> a / b;
        };
    }

    private static int compareLabels(TreeMap<String, String> a, TreeMap<String, String> b) {
        var ia = a.entrySet().iterator();
        var ib = b.entrySet().iterator();
        while (ia.hasNext() && ib.hasNext()) {
            var ea = ia.next();
            var eb = ib.next();
            int c = ea.getKey().compareTo(eb.getKey());
            if (c != 0) {
                return c;
            }
            c = ea.getValue().compareTo(eb.getValue());
            if (c != 0) {
                return c;
            }
        }
        return Integer.compare(a.size(), b.size());
    }
}
