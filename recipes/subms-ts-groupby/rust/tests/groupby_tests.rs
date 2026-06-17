use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::TsExpr;
use subms_ts_groupby::{GroupByError, group_by, sort_by, top_k, unique, value_counts};

// Build a dense frame from typed f64 columns: each column has a point at every
// ts in 0..n.
fn f64_frame(cols: &[(&str, &[f64])]) -> TsDataFrame {
    let mut f = TsDataFrame::new();
    for (name, vals) in cols {
        let mut s = TsSeries::<f64>::new();
        for (i, v) in vals.iter().enumerate() {
            s.push(i as i64, *v).unwrap();
        }
        f.push_column(*name, TsColumn::F64(s)).unwrap();
    }
    f
}

fn str_col(name: &str, vals: &[&str]) -> (String, TsColumn) {
    let mut s = TsSeries::<String>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, v.to_string()).unwrap();
    }
    (name.to_string(), TsColumn::Str(s))
}

fn f64_col(name: &str, vals: &[f64]) -> (String, TsColumn) {
    let mut s = TsSeries::<f64>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, *v).unwrap();
    }
    (name.to_string(), TsColumn::F64(s))
}

fn i64_col(name: &str, vals: &[i64]) -> (String, TsColumn) {
    let mut s = TsSeries::<i64>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, *v).unwrap();
    }
    (name.to_string(), TsColumn::I64(s))
}

fn frame(cols: Vec<(String, TsColumn)>) -> TsDataFrame {
    let mut f = TsDataFrame::new();
    for (name, col) in cols {
        f.push_column(name, col).unwrap();
    }
    f
}

fn f64_of(v: Option<TsValue>) -> Option<f64> {
    match v {
        Some(TsValue::F64(x)) => Some(x),
        _ => None,
    }
}

fn i64_of(v: Option<TsValue>) -> Option<i64> {
    match v {
        Some(TsValue::I64(x)) => Some(x),
        _ => None,
    }
}

fn str_of(v: Option<TsValue>) -> Option<String> {
    match v {
        Some(TsValue::Str(x)) => Some(x),
        _ => None,
    }
}

// HEADLINE: group by a STRING key with sum/mean/min/max/count vs a hand-rolled
// reference. Proves string keying works end to end.
#[test]
fn string_key_all_aggregations_match_reference() {
    let f = frame(vec![
        str_col(
            "symbol",
            &["AAPL", "MSFT", "AAPL", "MSFT", "AAPL", "GOOG"],
        ),
        f64_col("px", &[10.0, 20.0, 30.0, 40.0, 50.0, 7.0]),
    ]);
    let r = group_by(&f, &["symbol"])
        .unwrap()
        .agg(&[
            ("sum", TsExpr::col("px").sum()),
            ("mean", TsExpr::col("px").mean()),
            ("min", TsExpr::col("px").min()),
            ("max", TsExpr::col("px").max()),
            ("count", TsExpr::col("px").count()),
        ])
        .unwrap();

    // sorted by key: AAPL, GOOG, MSFT.
    assert_eq!(r.nrows(), 3);
    assert_eq!(str_of(r.value("symbol", 0)), Some("AAPL".to_string()));
    assert_eq!(str_of(r.value("symbol", 1)), Some("GOOG".to_string()));
    assert_eq!(str_of(r.value("symbol", 2)), Some("MSFT".to_string()));

    // AAPL: 10,30,50 ; GOOG: 7 ; MSFT: 20,40.
    assert_eq!(f64_of(r.value("sum", 0)), Some(90.0));
    assert_eq!(f64_of(r.value("mean", 0)), Some(30.0));
    assert_eq!(f64_of(r.value("min", 0)), Some(10.0));
    assert_eq!(f64_of(r.value("max", 0)), Some(50.0));
    assert_eq!(i64_of(r.value("count", 0)), Some(3));

    assert_eq!(f64_of(r.value("sum", 1)), Some(7.0));
    assert_eq!(i64_of(r.value("count", 1)), Some(1));

    assert_eq!(f64_of(r.value("sum", 2)), Some(60.0));
    assert_eq!(f64_of(r.value("mean", 2)), Some(30.0));
    assert_eq!(i64_of(r.value("count", 2)), Some(2));
}

#[test]
fn i64_key_groups_on_integer() {
    // an integer id / date-as-nanos key column.
    let f = frame(vec![
        i64_col("day", &[1, 2, 1, 2, 1]),
        f64_col("v", &[10.0, 20.0, 30.0, 40.0, 50.0]),
    ]);
    let r = group_by(&f, &["day"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    assert_eq!(r.nrows(), 2);
    assert_eq!(i64_of(r.value("day", 0)), Some(1));
    assert_eq!(f64_of(r.value("s", 0)), Some(90.0)); // 10+30+50
    assert_eq!(i64_of(r.value("day", 1)), Some(2));
    assert_eq!(f64_of(r.value("s", 1)), Some(60.0)); // 20+40
}

#[test]
fn multi_key_string_plus_int() {
    let f = frame(vec![
        str_col("sym", &["A", "A", "B", "A", "B"]),
        i64_col("side", &[0, 1, 0, 0, 0]),
        f64_col("v", &[1.0, 2.0, 3.0, 4.0, 5.0]),
    ]);
    let r = group_by(&f, &["sym", "side"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    // tuples: (A,0),(A,1),(B,0),(A,0),(B,0) -> groups (A,0):1+4=5, (A,1):2, (B,0):3+5=8.
    assert_eq!(r.nrows(), 3);
    assert_eq!(str_of(r.value("sym", 0)), Some("A".to_string()));
    assert_eq!(i64_of(r.value("side", 0)), Some(0));
    assert_eq!(f64_of(r.value("s", 0)), Some(5.0));
    assert_eq!(str_of(r.value("sym", 1)), Some("A".to_string()));
    assert_eq!(i64_of(r.value("side", 1)), Some(1));
    assert_eq!(f64_of(r.value("s", 1)), Some(2.0));
    assert_eq!(str_of(r.value("sym", 2)), Some("B".to_string()));
    assert_eq!(f64_of(r.value("s", 2)), Some(8.0));
}

#[test]
fn agg_with_computed_expr_per_group() {
    // sum(price * volume) per group = a notional sum.
    let f = frame(vec![
        str_col("sym", &["X", "X", "Y"]),
        f64_col("price", &[2.0, 3.0, 4.0]),
        f64_col("volume", &[5.0, 7.0, 6.0]),
    ]);
    let r = group_by(&f, &["sym"])
        .unwrap()
        .agg(&[(
            "notional",
            TsExpr::col("price").mul(TsExpr::col("volume")).sum(),
        )])
        .unwrap();
    // X: 2*5 + 3*7 = 31 ; Y: 4*6 = 24.
    assert_eq!(f64_of(r.value("notional", 0)), Some(31.0));
    assert_eq!(f64_of(r.value("notional", 1)), Some(24.0));
}

#[test]
fn empty_frame_yields_no_groups() {
    let f = frame(vec![
        ("k".to_string(), TsColumn::Str(TsSeries::<String>::new())),
        ("v".to_string(), TsColumn::F64(TsSeries::<f64>::new())),
    ]);
    let r = group_by(&f, &["k"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    assert_eq!(r.nrows(), 0);
    assert_eq!(r.ncols(), 2);
}

#[test]
fn single_group_when_key_is_constant() {
    let f = frame(vec![
        str_col("k", &["one", "one", "one"]),
        f64_col("v", &[1.0, 2.0, 3.0]),
    ]);
    let r = group_by(&f, &["k"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    assert_eq!(r.nrows(), 1);
    assert_eq!(str_of(r.value("k", 0)), Some("one".to_string()));
    assert_eq!(f64_of(r.value("s", 0)), Some(6.0));
}

#[test]
fn value_counts_sorted_by_descending_count() {
    let f = frame(vec![str_col("k", &["a", "b", "a", "a", "b", "c"])]);
    let vc = value_counts(&f, "k").unwrap();
    // a:3, b:2, c:1.
    assert_eq!(vc.nrows(), 2 + 1);
    assert_eq!(str_of(vc.value("k", 0)), Some("a".to_string()));
    assert_eq!(i64_of(vc.value("count", 0)), Some(3));
    assert_eq!(str_of(vc.value("k", 1)), Some("b".to_string()));
    assert_eq!(i64_of(vc.value("count", 1)), Some(2));
    assert_eq!(str_of(vc.value("k", 2)), Some("c".to_string()));
    assert_eq!(i64_of(vc.value("count", 2)), Some(1));
}

#[test]
fn unique_distinct_key_tuples_sorted() {
    let f = frame(vec![
        str_col("a", &["x", "x", "y", "x"]),
        i64_col("b", &[9, 9, 8, 7]),
    ]);
    let u = unique(&f, &["a", "b"]).unwrap();
    // distinct (a,b): (x,9),(y,8),(x,7) sorted -> (x,7),(x,9),(y,8).
    assert_eq!(u.nrows(), 3);
    assert_eq!(str_of(u.value("a", 0)), Some("x".to_string()));
    assert_eq!(i64_of(u.value("b", 0)), Some(7));
    assert_eq!(str_of(u.value("a", 1)), Some("x".to_string()));
    assert_eq!(i64_of(u.value("b", 1)), Some(9));
    assert_eq!(str_of(u.value("a", 2)), Some("y".to_string()));
    assert_eq!(i64_of(u.value("b", 2)), Some(8));
}

#[test]
fn top_k_returns_k_largest_rows() {
    let f = f64_frame(&[("v", &[3.0, 9.0, 1.0, 7.0, 5.0]), ("id", &[0.0, 1.0, 2.0, 3.0, 4.0])]);
    let top = top_k(&f, "v", 2).unwrap();
    // a reordered frame with the 2 largest v rows: v=9 (id 1), v=7 (id 3).
    let v = top.column("v").unwrap();
    let id = top.column("id").unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v.get(0), Some(TsValue::F64(9.0)));
    assert_eq!(id.get(0), Some(TsValue::F64(1.0)));
    assert_eq!(v.get(1), Some(TsValue::F64(7.0)));
    assert_eq!(id.get(1), Some(TsValue::F64(3.0)));
}

#[test]
fn top_k_clamps_to_available_rows() {
    let f = f64_frame(&[("v", &[1.0, 2.0])]);
    let top = top_k(&f, "v", 10).unwrap();
    let v = top.column("v").unwrap();
    assert_eq!(v.len(), 2);
    assert_eq!(v.get(0), Some(TsValue::F64(2.0)));
    assert_eq!(v.get(1), Some(TsValue::F64(1.0)));
}

#[test]
fn sort_by_ascending_and_descending() {
    let f = f64_frame(&[("v", &[3.0, 1.0, 2.0]), ("id", &[10.0, 11.0, 12.0])]);
    let asc = sort_by(&f, &["v"], true).unwrap();
    let av = asc.column("v").unwrap();
    let aid = asc.column("id").unwrap();
    assert_eq!(av.get(0), Some(TsValue::F64(1.0)));
    assert_eq!(aid.get(0), Some(TsValue::F64(11.0)));
    assert_eq!(av.get(2), Some(TsValue::F64(3.0)));

    let desc = sort_by(&f, &["v"], false).unwrap();
    let dv = desc.column("v").unwrap();
    assert_eq!(dv.get(0), Some(TsValue::F64(3.0)));
    assert_eq!(dv.get(2), Some(TsValue::F64(1.0)));
}

#[test]
fn sort_by_multi_key_lexicographic_on_string_then_int() {
    let f = frame(vec![
        str_col("sym", &["B", "A", "A"]),
        i64_col("seq", &[1, 5, 2]),
    ]);
    let s = sort_by(&f, &["sym", "seq"], true).unwrap();
    let sym = s.column("sym").unwrap();
    let seq = s.column("seq").unwrap();
    // sorted by sym then seq: (A,2),(A,5),(B,1).
    assert_eq!(sym.get(0), Some(TsValue::Str("A".to_string())));
    assert_eq!(seq.get(0), Some(TsValue::I64(2)));
    assert_eq!(sym.get(1), Some(TsValue::Str("A".to_string())));
    assert_eq!(seq.get(1), Some(TsValue::I64(5)));
    assert_eq!(sym.get(2), Some(TsValue::Str("B".to_string())));
    assert_eq!(seq.get(2), Some(TsValue::I64(1)));
}

#[test]
fn null_key_rows_are_dropped() {
    // symbol has a hole at ts=1 (size carries it). That row has a null key and
    // must not form a group or contribute to any aggregate.
    let mut symbol = TsSeries::<String>::new();
    symbol.push(0, "A".to_string()).unwrap();
    symbol.push(2, "A".to_string()).unwrap();
    symbol.push(3, "B".to_string()).unwrap();
    let mut size = TsSeries::<f64>::new();
    for (ts, v) in [(0i64, 10.0), (1, 99.0), (2, 20.0), (3, 5.0)] {
        size.push(ts, v).unwrap();
    }
    let f = frame(vec![
        ("symbol".to_string(), TsColumn::Str(symbol)),
        ("size".to_string(), TsColumn::F64(size)),
    ]);
    let r = group_by(&f, &["symbol"])
        .unwrap()
        .agg(&[("s", TsExpr::col("size").sum())])
        .unwrap();
    // A -> ts0,ts2 = 10+20 = 30 ; B -> ts3 = 5. ts1 (null key) dropped.
    assert_eq!(r.nrows(), 2);
    assert_eq!(f64_of(r.value("s", 0)), Some(30.0));
    assert_eq!(f64_of(r.value("s", 1)), Some(5.0));
}

#[test]
fn group_order_is_deterministic_regardless_of_input_order() {
    let f1 = frame(vec![
        str_col("k", &["c", "a", "b"]),
        f64_col("v", &[1.0, 1.0, 1.0]),
    ]);
    let f2 = frame(vec![
        str_col("k", &["a", "b", "c"]),
        f64_col("v", &[1.0, 1.0, 1.0]),
    ]);
    let r1 = group_by(&f1, &["k"])
        .unwrap()
        .agg(&[("c", TsExpr::col("v").count())])
        .unwrap();
    let r2 = group_by(&f2, &["k"])
        .unwrap()
        .agg(&[("c", TsExpr::col("v").count())])
        .unwrap();
    assert_eq!(r1, r2);
    assert_eq!(str_of(r1.value("k", 0)), Some("a".to_string()));
    assert_eq!(str_of(r1.value("k", 1)), Some("b".to_string()));
    assert_eq!(str_of(r1.value("k", 2)), Some("c".to_string()));
}

#[test]
fn mean_over_valid_only_with_a_gap() {
    // group A's v column has a gap at one of its rows; mean must divide by the
    // count of VALID cells, not the row count.
    let mut sym = TsSeries::<String>::new();
    for (ts, s) in [(0i64, "A"), (1, "A"), (2, "A"), (3, "B")] {
        sym.push(ts, s.to_string()).unwrap();
    }
    let mut v = TsSeries::<f64>::new();
    // A rows at ts0,ts2 (ts1 is a gap); B row at ts3.
    for (ts, x) in [(0i64, 4.0), (2, 8.0), (3, 100.0)] {
        v.push(ts, x).unwrap();
    }
    let f = frame(vec![
        ("sym".to_string(), TsColumn::Str(sym)),
        ("v".to_string(), TsColumn::F64(v)),
    ]);
    let r = group_by(&f, &["sym"])
        .unwrap()
        .agg(&[
            ("mean", TsExpr::col("v").mean()),
            ("count", TsExpr::col("v").count()),
        ])
        .unwrap();
    // A: valid cells 4 and 8 (the ts1 gap excluded) -> mean 6, count 2.
    assert_eq!(f64_of(r.value("mean", 0)), Some(6.0));
    assert_eq!(i64_of(r.value("count", 0)), Some(2));
    assert_eq!(f64_of(r.value("mean", 1)), Some(100.0));
    assert_eq!(i64_of(r.value("count", 1)), Some(1));
}

#[test]
fn agg_over_group_with_all_null_target_is_null_cell() {
    // group key present, but the aggregated column is null on every row of one
    // group -> Mean reduces to NaN -> surfaced as a null cell; Count is 0.
    let mut k = TsSeries::<String>::new();
    k.push(0, "a".to_string()).unwrap();
    k.push(1, "b".to_string()).unwrap();
    let mut v = TsSeries::<f64>::new();
    v.push(0, 5.0).unwrap(); // v only present at ts0 (group a); group b has no v.
    let f = frame(vec![
        ("k".to_string(), TsColumn::Str(k)),
        ("v".to_string(), TsColumn::F64(v)),
    ]);
    let r = group_by(&f, &["k"])
        .unwrap()
        .agg(&[
            ("m", TsExpr::col("v").mean()),
            ("c", TsExpr::col("v").count()),
        ])
        .unwrap();
    assert_eq!(f64_of(r.value("m", 0)), Some(5.0));
    assert_eq!(i64_of(r.value("c", 0)), Some(1));
    // group b: no v -> mean NaN -> null cell ; count 0.
    assert_eq!(r.value("m", 1), None);
    assert_eq!(i64_of(r.value("c", 1)), Some(0));
}

#[test]
fn empty_keys_errors() {
    let f = frame(vec![str_col("k", &["a"])]);
    assert_eq!(group_by(&f, &[]).unwrap_err(), GroupByError::NoKeys);
}

#[test]
fn unknown_key_column_errors() {
    let f = frame(vec![str_col("k", &["a"])]);
    assert_eq!(
        group_by(&f, &["nope"]).unwrap_err(),
        GroupByError::UnknownColumn("nope".to_string())
    );
}

#[test]
fn non_agg_expr_rejected() {
    let f = frame(vec![str_col("k", &["a"]), f64_col("v", &[2.0])]);
    let err = group_by(&f, &["k"])
        .unwrap()
        .agg(&[("bad", TsExpr::col("v"))])
        .unwrap_err();
    assert_eq!(err, GroupByError::NotAnAggregation("bad".to_string()));
}

fn bool_col(name: &str, vals: &[bool]) -> (String, TsColumn) {
    let mut s = TsSeries::<bool>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, *v).unwrap();
    }
    (name.to_string(), TsColumn::Bool(s))
}

fn bool_of(v: Option<TsValue>) -> Option<bool> {
    match v {
        Some(TsValue::Bool(x)) => Some(x),
        _ => None,
    }
}

#[test]
fn bool_key_groups_on_boolean() {
    let f = frame(vec![
        bool_col("flag", &[true, false, true, false]),
        f64_col("v", &[1.0, 2.0, 3.0, 4.0]),
    ]);
    let r = group_by(&f, &["flag"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    // sorted false < true: false -> 6, true -> 4.
    assert_eq!(r.nrows(), 2);
    assert_eq!(bool_of(r.value("flag", 0)), Some(false));
    assert_eq!(f64_of(r.value("s", 0)), Some(6.0));
    assert_eq!(bool_of(r.value("flag", 1)), Some(true));
    assert_eq!(f64_of(r.value("s", 1)), Some(4.0));
}

#[test]
fn f64_key_groups_on_double() {
    let f = f64_frame(&[("bucket", &[1.5, 2.5, 1.5]), ("v", &[10.0, 20.0, 30.0])]);
    let r = group_by(&f, &["bucket"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    assert_eq!(r.nrows(), 2);
    assert_eq!(f64_of(r.value("bucket", 0)), Some(1.5));
    assert_eq!(f64_of(r.value("s", 0)), Some(40.0));
    assert_eq!(f64_of(r.value("bucket", 1)), Some(2.5));
    assert_eq!(f64_of(r.value("s", 1)), Some(20.0));
}

#[test]
fn agg_min_max_over_string_column() {
    let f = frame(vec![
        str_col("grp", &["g", "g", "h"]),
        str_col("tag", &["delta", "alpha", "charlie"]),
    ]);
    let r = group_by(&f, &["grp"])
        .unwrap()
        .agg(&[
            ("lo", TsExpr::col("tag").min()),
            ("hi", TsExpr::col("tag").max()),
        ])
        .unwrap();
    assert_eq!(str_of(r.value("lo", 0)), Some("alpha".to_string()));
    assert_eq!(str_of(r.value("hi", 0)), Some("delta".to_string()));
    assert_eq!(str_of(r.value("lo", 1)), Some("charlie".to_string()));
    assert_eq!(str_of(r.value("hi", 1)), Some("charlie".to_string()));
}

#[test]
fn value_counts_on_i64_column() {
    let f = frame(vec![i64_col("id", &[7, 7, 9, 7, 9])]);
    let vc = value_counts(&f, "id").unwrap();
    assert_eq!(i64_of(vc.value("id", 0)), Some(7));
    assert_eq!(i64_of(vc.value("count", 0)), Some(3));
    assert_eq!(i64_of(vc.value("id", 1)), Some(9));
    assert_eq!(i64_of(vc.value("count", 1)), Some(2));
}

#[test]
fn value_counts_on_bool_column() {
    let f = frame(vec![bool_col("flag", &[true, true, false])]);
    let vc = value_counts(&f, "flag").unwrap();
    assert_eq!(vc.nrows(), 2);
    assert_eq!(bool_of(vc.value("flag", 0)), Some(true));
    assert_eq!(i64_of(vc.value("count", 0)), Some(2));
    assert_eq!(bool_of(vc.value("flag", 1)), Some(false));
}

#[test]
fn sort_by_single_i64_key_descending() {
    let f = frame(vec![i64_col("seq", &[3, 1, 2])]);
    let s = sort_by(&f, &["seq"], false).unwrap();
    let seq = s.column("seq").unwrap();
    assert_eq!(seq.get(0), Some(TsValue::I64(3)));
    assert_eq!(seq.get(1), Some(TsValue::I64(2)));
    assert_eq!(seq.get(2), Some(TsValue::I64(1)));
}

#[test]
fn top_k_on_i64_column() {
    let f = frame(vec![i64_col("score", &[30, 90, 10, 70])]);
    let top = top_k(&f, "score", 2).unwrap();
    let score = top.column("score").unwrap();
    assert_eq!(score.len(), 2);
    assert_eq!(score.get(0), Some(TsValue::I64(90)));
    assert_eq!(score.get(1), Some(TsValue::I64(70)));
}

#[test]
fn sort_by_empty_columns_errors() {
    let f = frame(vec![str_col("k", &["a"])]);
    assert_eq!(sort_by(&f, &[], true).err(), Some(GroupByError::NoKeys));
}

#[test]
fn top_k_unknown_column_errors() {
    let f = frame(vec![str_col("k", &["a"])]);
    assert_eq!(
        top_k(&f, "nope", 1).err(),
        Some(GroupByError::UnknownColumn("nope".to_string()))
    );
}

#[test]
fn sort_by_unknown_column_errors() {
    let f = frame(vec![str_col("k", &["a"])]);
    assert_eq!(
        sort_by(&f, &["nope"], true).err(),
        Some(GroupByError::UnknownColumn("nope".to_string()))
    );
}

#[test]
fn unknown_agg_column_errors() {
    let f = frame(vec![str_col("k", &["a", "b"]), f64_col("v", &[1.0, 2.0])]);
    let err = group_by(&f, &["k"])
        .unwrap()
        .agg(&[("s", TsExpr::col("ghost").sum())])
        .unwrap_err();
    assert_eq!(err, GroupByError::UnknownColumn("ghost".to_string()));
}

#[test]
fn group_by_accessors() {
    let f = frame(vec![str_col("k", &["a", "b"]), f64_col("v", &[1.0, 2.0])]);
    let gb = group_by(&f, &["k"]).unwrap();
    assert_eq!(gb.ngroups(), 2);
    assert_eq!(gb.key(0), vec![TsValue::Str("a".to_string())]);
    let r = gb.agg(&[("s", TsExpr::col("v").sum())]).unwrap();
    assert!(r.column("s").is_some());
    assert!(r.column("absent").is_none());
    assert_eq!(r.value("absent", 0), None);
}

#[test]
fn column_names_are_keys_then_aggs() {
    let f = frame(vec![str_col("k", &["a", "b"]), f64_col("v", &[1.0, 2.0])]);
    let r = group_by(&f, &["k"])
        .unwrap()
        .agg(&[("s", TsExpr::col("v").sum())])
        .unwrap();
    let names: Vec<&str> = r.column_names().collect();
    assert_eq!(names, vec!["k", "s"]);
}
