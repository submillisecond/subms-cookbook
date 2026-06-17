use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::TsExpr;
use subms_ts_lazy::{LazyTsFrame, PlanNode, node_cost_ns};

// A 8-row frame: px (f64), qty (i64), side (str). px is monotone, qty varies,
// side alternates buy/sell. The hand-rolled reference below mirrors it.
fn frame() -> TsDataFrame {
    let mut px = TsSeries::<f64>::new();
    let mut qty = TsSeries::<i64>::new();
    let mut side = TsSeries::<String>::new();
    let pxs = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0];
    let qtys = [5, 9, 2, 7, 4, 8, 1, 6];
    for i in 0..8usize {
        px.push(i as i64, pxs[i]).unwrap();
        qty.push(i as i64, qtys[i]).unwrap();
        let s = if i % 2 == 0 { "buy" } else { "sell" };
        side.push(i as i64, s.to_string()).unwrap();
    }
    TsDataFrame::new()
        .with_column("px", TsColumn::F64(px))
        .with_column("qty", TsColumn::I64(qty))
        .with_column("side", TsColumn::Str(side))
}

fn f64_col(result: &subms_ts_lazy::ResultFrame, name: &str) -> Vec<Option<f64>> {
    let arr = result.column(name).expect("column present");
    (0..result.nrows())
        .map(|i| match arr.get(i) {
            Some(TsValue::F64(v)) => Some(v),
            _ => None,
        })
        .collect()
}

fn i64_col(result: &subms_ts_lazy::ResultFrame, name: &str) -> Vec<Option<i64>> {
    let arr = result.column(name).expect("column present");
    (0..result.nrows())
        .map(|i| match arr.get(i) {
            Some(TsValue::I64(v)) => Some(v),
            _ => None,
        })
        .collect()
}

#[test]
fn pipeline_collect_matches_reference() {
    // filter px > 12 -> with_column gross = px*qty -> select [gross, px]
    //                -> sort gross asc -> limit 3.
    let result = LazyTsFrame::new(frame())
        .filter(TsExpr::col("px").gt(TsExpr::lit_f64(12.0)))
        .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
        .select(&["gross", "px"])
        .sort_by("gross", true)
        .limit(3)
        .collect()
        .unwrap();

    // Reference: rows with px > 12 are i=3..7 (px 13..17).
    // gross = 13*7=91, 14*4=56, 15*8=120, 16*1=16, 17*6=102.
    // sorted asc by gross: 16(px16), 56(px14), 91(px13), 102(px17), 120(px15).
    // limit 3: [16, 56, 91] with px [16, 14, 13].
    assert_eq!(result.nrows(), 3);
    assert_eq!(
        f64_col(&result, "gross"),
        vec![Some(16.0), Some(56.0), Some(91.0)]
    );
    assert_eq!(
        f64_col(&result, "px"),
        vec![Some(16.0), Some(14.0), Some(13.0)]
    );
    let names: Vec<&str> = result.column_names().collect();
    assert_eq!(names, vec!["gross", "px"]);
}

#[test]
fn agg_terminal_whole_frame() {
    let result = LazyTsFrame::new(frame())
        .agg(&[
            ("px_sum", TsExpr::col("px").sum()),
            ("qty_max", TsExpr::col("qty").max()),
            ("n", TsExpr::col("px").count()),
        ])
        .unwrap();
    assert_eq!(result.nrows(), 1);
    // px sum = 10+11+...+17 = 108.
    assert_eq!(f64_col(&result, "px_sum"), vec![Some(108.0)]);
    assert_eq!(i64_col(&result, "qty_max"), vec![Some(9)]);
    assert_eq!(i64_col(&result, "n"), vec![Some(8)]);
}

#[test]
fn optimise_preserves_results() {
    let build = || {
        LazyTsFrame::new(frame())
            .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
            .filter(TsExpr::col("px").gt(TsExpr::lit_f64(11.0)))
            .with_column("half", TsExpr::col("px").div(TsExpr::lit_f64(2.0)))
            .filter(TsExpr::col("qty").ge(TsExpr::lit_i64(4)))
            .select(&["gross", "half", "px"])
            .sort_by("gross", false)
    };
    let optimised = build().collect().unwrap();
    let raw = build().collect_unoptimised().unwrap();
    assert_eq!(optimised, raw);
}

#[test]
fn projection_pushdown_drops_unreferenced_column() {
    // Only px + qty are referenced downstream; side should be projected away by
    // an early Select that the optimiser inserts.
    let lazy = LazyTsFrame::new(frame())
        .filter(TsExpr::col("px").gt(TsExpr::lit_f64(10.0)))
        .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
        .select(&["gross"]);
    let plan = lazy.explain();
    // The first node after optimise is an inserted Select over the live source
    // columns (px, qty) - and it must NOT carry `side`.
    assert!(plan.contains("Select [px, qty]"), "plan was:\n{plan}");
    assert!(
        !plan.contains("side"),
        "side should be projected away:\n{plan}"
    );
}

#[test]
fn predicate_pushdown_reorders_filter_above_with_column() {
    // The filter reads only px; it can slide above the with_column that derives
    // `gross`. After optimise the Filter must appear before the WithColumn.
    let lazy = LazyTsFrame::new(frame())
        .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
        .filter(TsExpr::col("px").gt(TsExpr::lit_f64(12.0)));
    let raw = lazy.explain_unoptimised();
    let opt = lazy.explain();
    let raw_wc = raw.find("WithColumn").unwrap();
    let raw_f = raw.find("Filter").unwrap();
    assert!(raw_wc < raw_f, "unoptimised has WithColumn before Filter");
    let opt_f = opt.find("Filter").unwrap();
    let opt_wc = opt.find("WithColumn").unwrap();
    assert!(
        opt_f < opt_wc,
        "optimised pushes Filter above WithColumn:\n{opt}"
    );
}

#[test]
fn predicate_pushdown_blocked_when_filter_reads_derived_column() {
    // The filter reads `gross`, which the with_column derives - it must NOT move
    // above that with_column.
    let lazy = LazyTsFrame::new(frame())
        .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
        .filter(TsExpr::col("gross").gt(TsExpr::lit_f64(100.0)));
    let opt = lazy.explain();
    let opt_wc = opt.find("WithColumn").unwrap();
    let opt_f = opt.find("Filter").unwrap();
    assert!(
        opt_wc < opt_f,
        "filter on derived column stays below it:\n{opt}"
    );
}

#[test]
fn filter_on_string_column() {
    let result = LazyTsFrame::new(frame())
        .filter(TsExpr::col("side").eq(TsExpr::lit_str("buy")))
        .select(&["px"])
        .collect()
        .unwrap();
    // buys are even indices: px 10, 12, 14, 16.
    assert_eq!(result.nrows(), 4);
    assert_eq!(
        f64_col(&result, "px"),
        vec![Some(10.0), Some(12.0), Some(14.0), Some(16.0)]
    );
}

#[test]
fn with_column_computed_expr() {
    let result = LazyTsFrame::new(frame())
        .with_column("bumped", TsExpr::col("px").add(TsExpr::lit_f64(100.0)))
        .select(&["bumped"])
        .limit(2)
        .collect()
        .unwrap();
    assert_eq!(f64_col(&result, "bumped"), vec![Some(110.0), Some(111.0)]);
}

#[test]
fn sort_ascending_and_descending() {
    let asc = LazyTsFrame::new(frame())
        .sort_by("qty", true)
        .select(&["qty"])
        .collect()
        .unwrap();
    assert_eq!(
        i64_col(&asc, "qty"),
        vec![
            Some(1),
            Some(2),
            Some(4),
            Some(5),
            Some(6),
            Some(7),
            Some(8),
            Some(9)
        ]
    );
    let desc = LazyTsFrame::new(frame())
        .sort_by("qty", false)
        .select(&["qty"])
        .collect()
        .unwrap();
    assert_eq!(
        i64_col(&desc, "qty"),
        vec![
            Some(9),
            Some(8),
            Some(7),
            Some(6),
            Some(5),
            Some(4),
            Some(2),
            Some(1)
        ]
    );
}

#[test]
fn limit_truncates() {
    let result = LazyTsFrame::new(frame())
        .select(&["px"])
        .limit(3)
        .collect()
        .unwrap();
    assert_eq!(result.nrows(), 3);
    let big = LazyTsFrame::new(frame())
        .select(&["px"])
        .limit(999)
        .collect()
        .unwrap();
    assert_eq!(big.nrows(), 8);
}

#[test]
fn empty_pipeline_equals_source() {
    let result = LazyTsFrame::new(frame()).collect().unwrap();
    assert_eq!(result.nrows(), 8);
    let names: Vec<&str> = result.column_names().collect();
    assert_eq!(names, vec!["px", "qty", "side"]);
    // Round-trip to a TsDataFrame matches the source shape.
    let df = result.into_data_frame();
    let df_names: Vec<&str> = df.column_names().collect();
    assert_eq!(df_names, vec!["px", "qty", "side"]);
}

#[test]
fn explain_renders_ops_in_order() {
    let plan = LazyTsFrame::new(frame())
        .filter(TsExpr::col("px").gt(TsExpr::lit_f64(11.0)))
        .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
        .sort_by("gross", true)
        .limit(2)
        .select(&["gross"])
        .explain_unoptimised();
    let f = plan.find("Filter").unwrap();
    let wc = plan.find("WithColumn gross").unwrap();
    let sort = plan.find("SortBy gross asc").unwrap();
    let limit = plan.find("Limit 2").unwrap();
    let sel = plan.find("Select [gross]").unwrap();
    assert!(
        f < wc && wc < sort && sort < limit && limit < sel,
        "plan:\n{plan}"
    );
}

#[test]
fn certify_total_is_sum_of_node_costs_plus_overhead() {
    let lazy = LazyTsFrame::new(frame())
        .filter(TsExpr::col("px").gt(TsExpr::lit_f64(11.0)))
        .with_column("gross", TsExpr::col("px").mul(TsExpr::col("qty")))
        .select(&["gross"]);
    let plan = lazy.build_plan();
    let cert = lazy.certify("ci-dedicated", 0);

    // total = overhead + sum of per-node costs over the OPTIMISED node list.
    let expected: u64 =
        plan.planner_overhead_ns() + plan.stages().iter().map(|s| s.p99_ns).sum::<u64>();
    assert_eq!(cert.total_p99_ns, expected);
    // Each stage's p99 equals the node cost model.
    assert!(cert.verify());
    assert!(cert.meets_budget(10_000_000));
    assert_eq!(cert.hardware_tier, "ci-dedicated");
}

#[test]
fn certify_node_costs_match_model() {
    let select = PlanNode::Select(vec!["a".into()]);
    let filter = PlanNode::Filter(TsExpr::col("a").gt(TsExpr::lit_f64(0.0)));
    let agg2 = PlanNode::Agg(vec![
        ("s".into(), TsExpr::col("a").sum()),
        ("m".into(), TsExpr::col("a").mean()),
    ]);
    // The agg cost scales with the number of output aggregates.
    assert_eq!(
        node_cost_ns(&agg2),
        2 * node_cost_ns(&PlanNode::Agg(vec![("s".into(), TsExpr::col("a").sum())]))
    );
    assert!(node_cost_ns(&filter) > node_cost_ns(&select));
}

#[test]
fn empty_plan_certifies_overhead_only() {
    let cert = LazyTsFrame::new(frame()).certify("laptop", 0);
    assert_eq!(cert.total_p99_ns, subms_ts_lazy::PLANNER_OVERHEAD_NS);
    assert!(cert.verify());
}

#[test]
fn unknown_sort_column_errors() {
    let err = LazyTsFrame::new(frame()).sort_by("missing", true).collect();
    assert!(matches!(
        err,
        Err(subms_ts_lazy::LazyError::UnknownSortColumn(_))
    ));
}
