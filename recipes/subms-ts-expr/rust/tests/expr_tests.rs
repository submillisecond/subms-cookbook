use subms_ts::{TsColumn, TsDataFrame, TsDataType, TsSeries, TsValue};
use subms_ts_expr::{TsArray, TsExpr, TsExprError, eval, eval_scalar, when};

fn f64_col(pts: &[(i64, f64)]) -> TsColumn {
    let mut s = TsSeries::<f64>::new();
    for (ts, v) in pts {
        s.push(*ts, *v).unwrap();
    }
    TsColumn::F64(s)
}

fn i64_col(pts: &[(i64, i64)]) -> TsColumn {
    let mut s = TsSeries::<i64>::new();
    for (ts, v) in pts {
        s.push(*ts, *v).unwrap();
    }
    TsColumn::I64(s)
}

fn bool_col(pts: &[(i64, bool)]) -> TsColumn {
    let mut s = TsSeries::<bool>::new();
    for (ts, v) in pts {
        s.push(*ts, *v).unwrap();
    }
    TsColumn::Bool(s)
}

fn str_col(pts: &[(i64, &str)]) -> TsColumn {
    let mut s = TsSeries::<String>::new();
    for (ts, v) in pts {
        s.push(*ts, v.to_string()).unwrap();
    }
    TsColumn::Str(s)
}

fn pairs(a: &TsArray) -> Vec<Option<TsValue>> {
    (0..a.len()).map(|i| a.get(i)).collect()
}

fn f(values: &[f64]) -> Vec<Option<TsValue>> {
    values.iter().map(|&v| Some(TsValue::F64(v))).collect()
}

#[test]
fn col_pulls_each_column_type() {
    let frame = TsDataFrame::new()
        .with_column("d", f64_col(&[(0, 1.5), (1, 2.5)]))
        .with_column("i", i64_col(&[(0, 7), (1, 8)]))
        .with_column("b", bool_col(&[(0, true), (1, false)]))
        .with_column("s", str_col(&[(0, "x"), (1, "y")]));

    let d = eval(&TsExpr::col("d"), &frame).unwrap();
    assert_eq!(d.data_type(), TsDataType::F64);
    assert_eq!(pairs(&d), vec![Some(TsValue::F64(1.5)), Some(TsValue::F64(2.5))]);

    let i = eval(&TsExpr::col("i"), &frame).unwrap();
    assert_eq!(i.data_type(), TsDataType::I64);
    assert_eq!(pairs(&i), vec![Some(TsValue::I64(7)), Some(TsValue::I64(8))]);

    let b = eval(&TsExpr::col("b"), &frame).unwrap();
    assert_eq!(b.data_type(), TsDataType::Bool);
    assert_eq!(
        pairs(&b),
        vec![Some(TsValue::Bool(true)), Some(TsValue::Bool(false))]
    );

    let s = eval(&TsExpr::col("s"), &frame).unwrap();
    assert_eq!(s.data_type(), TsDataType::Str);
    assert_eq!(
        pairs(&s),
        vec![
            Some(TsValue::Str("x".into())),
            Some(TsValue::Str("y".into()))
        ]
    );
}

#[test]
fn arithmetic_f64_and_i64_stay_typed() {
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 2.0), (1, 3.0)]))
        .with_column("b", f64_col(&[(0, 5.0), (1, 7.0)]));
    let add = eval(&TsExpr::col("a").add(TsExpr::col("b")), &frame).unwrap();
    assert_eq!(add.data_type(), TsDataType::F64);
    assert_eq!(pairs(&add), f(&[7.0, 10.0]));

    let ints = TsDataFrame::new()
        .with_column("a", i64_col(&[(0, 10), (1, 4)]))
        .with_column("b", i64_col(&[(0, 3), (1, 6)]));
    let mul = eval(&TsExpr::col("a").mul(TsExpr::col("b")), &ints).unwrap();
    assert_eq!(mul.data_type(), TsDataType::I64);
    assert_eq!(
        pairs(&mul),
        vec![Some(TsValue::I64(30)), Some(TsValue::I64(24))]
    );
}

#[test]
fn arithmetic_mixed_promotes_to_f64() {
    let frame = TsDataFrame::new()
        .with_column("d", f64_col(&[(0, 2.5), (1, 4.0)]))
        .with_column("i", i64_col(&[(0, 2), (1, 3)]));
    let r = eval(&TsExpr::col("d").add(TsExpr::col("i")), &frame).unwrap();
    assert_eq!(r.data_type(), TsDataType::F64);
    assert_eq!(pairs(&r), f(&[4.5, 7.0]));
    // i64 + f64 literal also promotes.
    let r2 = eval(&TsExpr::col("i").mul(TsExpr::lit_f64(1.5)), &frame).unwrap();
    assert_eq!(r2.data_type(), TsDataType::F64);
    assert_eq!(pairs(&r2), f(&[3.0, 4.5]));
}

#[test]
fn div_by_zero_is_null_f64_and_i64() {
    let df = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 10.0), (1, 6.0)]))
        .with_column("b", f64_col(&[(0, 2.0), (1, 0.0)]));
    let c = eval(&TsExpr::col("a").div(TsExpr::col("b")), &df).unwrap();
    assert_eq!(pairs(&c), vec![Some(TsValue::F64(5.0)), None]);

    let di = TsDataFrame::new()
        .with_column("a", i64_col(&[(0, 10), (1, 6)]))
        .with_column("b", i64_col(&[(0, 2), (1, 0)]));
    let ci = eval(&TsExpr::col("a").div(TsExpr::col("b")), &di).unwrap();
    assert_eq!(pairs(&ci), vec![Some(TsValue::I64(5)), None]);
}

#[test]
fn compare_numeric_with_promotion_yields_bool() {
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 1.0), (1, 5.0), (2, 5.0)]))
        .with_column("i", i64_col(&[(0, 2), (1, 4), (2, 5)]));
    let gt = eval(&TsExpr::col("a").gt(TsExpr::col("i")), &frame).unwrap();
    assert_eq!(gt.data_type(), TsDataType::Bool);
    assert_eq!(
        pairs(&gt),
        vec![
            Some(TsValue::Bool(false)),
            Some(TsValue::Bool(true)),
            Some(TsValue::Bool(false))
        ]
    );
    let ge = eval(&TsExpr::col("a").ge(TsExpr::col("i")), &frame).unwrap();
    assert_eq!(
        pairs(&ge),
        vec![
            Some(TsValue::Bool(false)),
            Some(TsValue::Bool(true)),
            Some(TsValue::Bool(true))
        ]
    );
}

#[test]
fn compare_str_eq_and_ord() {
    let frame = TsDataFrame::new()
        .with_column("a", str_col(&[(0, "abc"), (1, "xyz"), (2, "m")]))
        .with_column("b", str_col(&[(0, "abc"), (1, "abc"), (2, "z")]));
    let eqm = eval(&TsExpr::col("a").eq(TsExpr::col("b")), &frame).unwrap();
    assert_eq!(
        pairs(&eqm),
        vec![
            Some(TsValue::Bool(true)),
            Some(TsValue::Bool(false)),
            Some(TsValue::Bool(false))
        ]
    );
    let lt = eval(&TsExpr::col("a").lt(TsExpr::col("b")), &frame).unwrap();
    // "abc"<"abc" false; "xyz"<"abc" false; "m"<"z" true.
    assert_eq!(
        pairs(&lt),
        vec![
            Some(TsValue::Bool(false)),
            Some(TsValue::Bool(false)),
            Some(TsValue::Bool(true))
        ]
    );
}

#[test]
fn when_selects_elementwise_keeping_type() {
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 1.0), (1, 9.0)]))
        .with_column("b", f64_col(&[(0, 2.0), (1, 3.0)]));
    let e = when(
        TsExpr::col("a").gt(TsExpr::col("b")),
        TsExpr::col("a"),
        TsExpr::col("b"),
    );
    let c = eval(&e, &frame).unwrap();
    assert_eq!(c.data_type(), TsDataType::F64);
    // row0: a<b -> b=2; row1: a>b -> a=9.
    assert_eq!(pairs(&c), f(&[2.0, 9.0]));
}

#[test]
fn agg_sum_mean_min_max_count_over_valid_with_gap() {
    // a has a hole at ts=1 (b carries it); reductions skip the gap.
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 1.0), (2, 3.0), (3, 4.0)]))
        .with_column("b", f64_col(&[(0, 9.0), (1, 8.0), (2, 7.0), (3, 6.0)]));
    assert_eq!(
        eval_scalar(&TsExpr::col("a").sum(), &frame).unwrap(),
        TsValue::F64(8.0)
    );
    assert_eq!(
        eval_scalar(&TsExpr::col("a").mean(), &frame).unwrap(),
        TsValue::F64(8.0 / 3.0)
    );
    assert_eq!(
        eval_scalar(&TsExpr::col("a").min(), &frame).unwrap(),
        TsValue::F64(1.0)
    );
    assert_eq!(
        eval_scalar(&TsExpr::col("a").max(), &frame).unwrap(),
        TsValue::F64(4.0)
    );
    // Count is the number of VALID cells over the 4-row union axis = 3.
    assert_eq!(
        eval_scalar(&TsExpr::col("a").count(), &frame).unwrap(),
        TsValue::I64(3)
    );
}

#[test]
fn agg_min_max_keep_operand_type() {
    let ints = TsDataFrame::new().with_column("a", i64_col(&[(0, 5), (1, 2), (2, 9)]));
    assert_eq!(
        eval_scalar(&TsExpr::col("a").min(), &ints).unwrap(),
        TsValue::I64(2)
    );
    assert_eq!(
        eval_scalar(&TsExpr::col("a").max(), &ints).unwrap(),
        TsValue::I64(9)
    );
    let strs = TsDataFrame::new().with_column("a", str_col(&[(0, "b"), (1, "a"), (2, "c")]));
    assert_eq!(
        eval_scalar(&TsExpr::col("a").min(), &strs).unwrap(),
        TsValue::Str("a".into())
    );
    assert_eq!(
        eval_scalar(&TsExpr::col("a").max(), &strs).unwrap(),
        TsValue::Str("c".into())
    );
}

#[test]
fn null_propagates_through_binary() {
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 1.0), (2, 3.0)]))
        .with_column("b", f64_col(&[(0, 9.0), (1, 8.0), (2, 7.0)]));
    let c = eval(&TsExpr::col("a").add(TsExpr::col("b")), &frame).unwrap();
    // row for ts=1: a is null -> result null even though b is present.
    assert_eq!(
        pairs(&c),
        vec![Some(TsValue::F64(10.0)), None, Some(TsValue::F64(10.0))]
    );
}

#[test]
fn fill_null_and_drop_nulls() {
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 1.0), (2, 3.0)]))
        .with_column("b", f64_col(&[(0, 9.0), (1, 8.0), (2, 7.0)]));
    let c = eval(&TsExpr::col("a"), &frame).unwrap();
    let filled = c.fill_null(TsValue::F64(-1.0));
    assert_eq!(pairs(&filled), f(&[1.0, -1.0, 3.0]));
    assert!(filled.valid().iter().all(|&v| v));

    let dropped = c.drop_nulls();
    assert_eq!(dropped.len(), 2);
    assert_eq!(pairs(&dropped), f(&[1.0, 3.0]));
}

#[test]
fn unknown_column_errors() {
    let frame = TsDataFrame::new().with_column("a", f64_col(&[(0, 1.0)]));
    let err = eval(&TsExpr::col("nope"), &frame).unwrap_err();
    assert_eq!(err, TsExprError::UnknownColumn("nope".to_string()));
}

#[test]
fn type_mismatch_errors() {
    // arithmetic on a Str.
    let frame = TsDataFrame::new()
        .with_column("s", str_col(&[(0, "x")]))
        .with_column("d", f64_col(&[(0, 1.0)]));
    let err = eval(&TsExpr::col("s").add(TsExpr::col("d")), &frame).unwrap_err();
    assert!(matches!(err, TsExprError::TypeMismatch(_)));

    // when with disagreeing branch types.
    let err2 = eval(
        &when(
            TsExpr::col("d").gt(TsExpr::lit_f64(0.0)),
            TsExpr::col("d"),
            TsExpr::lit_str("nope"),
        ),
        &frame,
    )
    .unwrap_err();
    assert!(matches!(err2, TsExprError::TypeMismatch(_)));

    // non-Agg into eval_scalar.
    let err3 = eval_scalar(&TsExpr::col("d"), &frame).unwrap_err();
    assert_eq!(err3, TsExprError::NotScalar);
}

#[test]
fn unary_neg_abs_numeric_only() {
    let frame = TsDataFrame::new().with_column("a", f64_col(&[(0, -3.0), (1, 4.0)]));
    let neg = eval(&TsExpr::col("a").neg(), &frame).unwrap();
    assert_eq!(pairs(&neg), f(&[3.0, -4.0]));
    let abs = eval(&TsExpr::col("a").abs(), &frame).unwrap();
    assert_eq!(pairs(&abs), f(&[3.0, 4.0]));

    let strs = TsDataFrame::new().with_column("a", str_col(&[(0, "x")]));
    let err = eval(&TsExpr::col("a").neg(), &strs).unwrap_err();
    assert!(matches!(err, TsExprError::TypeMismatch(_)));
}

#[test]
fn deep_nested_expression_matches_hand_computed() {
    // when(close > open, (close - open) * 2, 0).mean(), with a gap in close.
    let open = &[(0i64, 5.0), (1, 2.0), (2, 8.0), (3, 4.0)];
    let close = &[(0i64, 3.0), (2, 1.0), (3, 9.0)]; // gap at ts=1
    let frame = TsDataFrame::new()
        .with_column("open", f64_col(open))
        .with_column("close", f64_col(close));
    let e = when(
        TsExpr::col("close").gt(TsExpr::col("open")),
        TsExpr::col("close").sub(TsExpr::col("open")).mul(TsExpr::lit_f64(2.0)),
        TsExpr::lit_f64(0.0),
    )
    .mean();

    // hand reference over the union axis {0,1,2,3}; ts=1 has no close -> cond
    // null -> when null -> excluded from the mean.
    let rows: &[(f64, Option<f64>)] = &[
        (5.0, Some(3.0)),
        (2.0, None),
        (8.0, Some(1.0)),
        (4.0, Some(9.0)),
    ];
    let mut vals = Vec::new();
    for (o, c) in rows {
        if let Some(cv) = c {
            vals.push(if cv > o { (cv - o) * 2.0 } else { 0.0 });
        }
    }
    let want = vals.iter().sum::<f64>() / vals.len() as f64;
    assert_eq!(eval_scalar(&e, &frame).unwrap(), TsValue::F64(want));
}

#[test]
fn when_with_null_cond_is_null() {
    let frame = TsDataFrame::new()
        .with_column("a", f64_col(&[(0, 1.0), (2, 1.0)]))
        .with_column("guard", bool_col(&[(0, true), (1, true), (2, true)]));
    // cond pulls a column with a gap at ts=1 -> cond null -> select null.
    let e = when(
        TsExpr::col("a").gt(TsExpr::lit_f64(0.0)),
        TsExpr::lit_f64(10.0),
        TsExpr::lit_f64(20.0),
    );
    let c = eval(&e, &frame).unwrap();
    assert_eq!(
        pairs(&c),
        vec![Some(TsValue::F64(10.0)), None, Some(TsValue::F64(10.0))]
    );
}

#[test]
fn empty_frame_aggs_have_defined_scalars() {
    let frame = TsDataFrame::new().with_column("a", TsColumn::F64(TsSeries::<f64>::new()));
    assert_eq!(
        eval_scalar(&TsExpr::col("a").count(), &frame).unwrap(),
        TsValue::I64(0)
    );
    assert_eq!(
        eval_scalar(&TsExpr::col("a").sum(), &frame).unwrap(),
        TsValue::F64(0.0)
    );
    match eval_scalar(&TsExpr::col("a").mean(), &frame).unwrap() {
        TsValue::F64(v) => assert!(v.is_nan()),
        other => panic!("expected NaN F64, got {other:?}"),
    }
    assert_eq!(
        eval_scalar(&TsExpr::col("a").min(), &frame).unwrap(),
        TsValue::Null
    );
}
