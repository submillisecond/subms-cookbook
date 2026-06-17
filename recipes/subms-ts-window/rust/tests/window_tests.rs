use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::{TsArray, TsExpr};
use subms_ts_window::{
    cummax, cummin, cumprod, cumsum, dense_rank, lag, lead, over, rank, row_number,
};

// Build a frame from typed column builders. Each builder owns its name + a
// (ts, value) point list; a missing ts in one column becomes a null cell on the
// aligned axis.
fn frame(cols: Vec<(String, TsColumn)>) -> TsDataFrame {
    let mut f = TsDataFrame::new();
    for (name, col) in cols {
        f.push_column(name, col).unwrap();
    }
    f
}

fn f64_col(name: &str, pts: &[(i64, f64)]) -> (String, TsColumn) {
    let mut s = TsSeries::<f64>::new();
    for &(ts, v) in pts {
        s.push(ts, v).unwrap();
    }
    (name.to_string(), TsColumn::F64(s))
}

fn i64_col(name: &str, pts: &[(i64, i64)]) -> (String, TsColumn) {
    let mut s = TsSeries::<i64>::new();
    for &(ts, v) in pts {
        s.push(ts, v).unwrap();
    }
    (name.to_string(), TsColumn::I64(s))
}

fn str_col(name: &str, pts: &[(i64, &str)]) -> (String, TsColumn) {
    let mut s = TsSeries::<String>::new();
    for &(ts, v) in pts {
        s.push(ts, v.to_string()).unwrap();
    }
    (name.to_string(), TsColumn::Str(s))
}

// Read an array's cells as f64 options (None where null), for numeric asserts.
fn f64s(a: &TsArray) -> Vec<Option<f64>> {
    (0..a.len())
        .map(|i| match a.get(i) {
            Some(TsValue::F64(x)) => Some(x),
            Some(TsValue::I64(x)) => Some(x as f64),
            _ => None,
        })
        .collect()
}

// Read an I64 array's cells as i64 options.
fn i64s(a: &TsArray) -> Vec<Option<i64>> {
    (0..a.len())
        .map(|i| match a.get(i) {
            Some(TsValue::I64(x)) => Some(x),
            _ => None,
        })
        .collect()
}

// f64-keyed two-partition frame: key 1.0 at even ts, 2.0 at odd ts; val == ts.
fn two_partition_frame() -> TsDataFrame {
    frame(vec![
        f64_col(
            "key",
            &[(0, 1.0), (1, 2.0), (2, 1.0), (3, 2.0), (4, 1.0), (5, 2.0)],
        ),
        f64_col(
            "val",
            &[(0, 0.0), (1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 5.0)],
        ),
    ])
}

// STRING-keyed two-partition frame: AAPL at even ts, MSFT at odd ts; px == ts.
fn two_symbol_frame() -> TsDataFrame {
    frame(vec![
        str_col(
            "sym",
            &[
                (0, "AAPL"),
                (1, "MSFT"),
                (2, "AAPL"),
                (3, "MSFT"),
                (4, "AAPL"),
                (5, "MSFT"),
            ],
        ),
        f64_col(
            "px",
            &[(0, 0.0), (1, 1.0), (2, 2.0), (3, 3.0), (4, 4.0), (5, 5.0)],
        ),
    ])
}

#[test]
fn lag_shifts_within_partition_with_null_head() {
    let f = two_partition_frame();
    let got = lag(&f, "val", 1, &["key"]).unwrap();
    // partition 1.0 rows: ts 0,2,4 -> vals 0,2,4; lag1 -> null,0,2
    // partition 2.0 rows: ts 1,3,5 -> vals 1,3,5; lag1 -> null,1,3
    assert_eq!(
        f64s(&got),
        vec![None, None, Some(0.0), Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn lead_shifts_within_partition_with_null_tail() {
    let f = two_partition_frame();
    let got = lead(&f, "val", 1, &["key"]).unwrap();
    // partition 1.0: vals 0,2,4 ; lead1 -> 2,4,null  (rows 0,2,4)
    // partition 2.0: vals 1,3,5 ; lead1 -> 3,5,null  (rows 1,3,5)
    assert_eq!(
        f64s(&got),
        vec![Some(2.0), Some(3.0), Some(4.0), Some(5.0), None, None]
    );
}

#[test]
fn lag_stays_within_two_string_partitions() {
    // The headline: typed STRING partition keys. If lag crossed partitions, the
    // row at ts=2 (AAPL) lag1 would pick ts=1's px (MSFT, 1.0); it must instead
    // pick AAPL's previous, ts=0 (0.0). Symmetric for MSFT.
    let f = two_symbol_frame();
    let got = lag(&f, "px", 1, &["sym"]).unwrap();
    assert_eq!(got.data_type(), subms_ts::TsDataType::F64);
    assert_eq!(
        f64s(&got),
        vec![None, None, Some(0.0), Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn lag_preserves_string_column_type() {
    // lag over a non-numeric column produces a same-typed array.
    let f = frame(vec![
        str_col("sym", &[(0, "AAPL"), (1, "AAPL"), (2, "AAPL")]),
        str_col("note", &[(0, "a"), (1, "b"), (2, "c")]),
    ]);
    let got = lag(&f, "note", 1, &["sym"]).unwrap();
    assert_eq!(got.data_type(), subms_ts::TsDataType::Str);
    assert_eq!(got.get(0), None);
    assert_eq!(got.get(1), Some(TsValue::Str("a".to_string())));
    assert_eq!(got.get(2), Some(TsValue::Str("b".to_string())));
}

#[test]
fn lag_n_greater_than_one() {
    let f = two_partition_frame();
    let got = lag(&f, "val", 2, &["key"]).unwrap();
    // partition 1.0 vals 0,2,4 ; lag2 -> null,null,0
    assert_eq!(got.get(0), None);
    assert_eq!(got.get(2), None);
    assert_eq!(got.get(4), Some(TsValue::F64(0.0)));
}

#[test]
fn row_number_is_one_to_k_per_partition() {
    let f = two_symbol_frame();
    let got = row_number(&f, &["sym"], None).unwrap();
    // arrival order within each partition: 1,2,3 each, interleaved on the axis.
    assert_eq!(got.data_type(), subms_ts::TsDataType::I64);
    assert_eq!(
        i64s(&got),
        vec![Some(1), Some(1), Some(2), Some(2), Some(3), Some(3)]
    );
}

#[test]
fn rank_handles_ties_with_gap() {
    // single partition, order_by has ties: values 10,10,20,30 over rows 0..3.
    let f = frame(vec![
        i64_col("key", &[(0, 1), (1, 1), (2, 1), (3, 1)]),
        f64_col("ord", &[(0, 10.0), (1, 10.0), (2, 20.0), (3, 30.0)]),
    ]);
    let got = rank(&f, &["key"], "ord").unwrap();
    // ranks: 1,1,3,4 (tie at rank 1 skips rank 2).
    assert_eq!(i64s(&got), vec![Some(1), Some(1), Some(3), Some(4)]);
}

#[test]
fn dense_rank_handles_ties_without_gap() {
    let f = frame(vec![
        i64_col("key", &[(0, 1), (1, 1), (2, 1), (3, 1)]),
        f64_col("ord", &[(0, 10.0), (1, 10.0), (2, 20.0), (3, 30.0)]),
    ]);
    let got = dense_rank(&f, &["key"], "ord").unwrap();
    // dense ranks: 1,1,2,3 (no gap).
    assert_eq!(i64s(&got), vec![Some(1), Some(1), Some(2), Some(3)]);
}

#[test]
fn cumsum_per_partition_matches_reference() {
    let f = two_partition_frame();
    let got = cumsum(&f, "val", &["key"], None).unwrap();
    // partition 1.0 vals 0,2,4 -> running 0,2,6 (rows 0,2,4)
    // partition 2.0 vals 1,3,5 -> running 1,4,9 (rows 1,3,5)
    assert_eq!(
        f64s(&got),
        vec![
            Some(0.0),
            Some(1.0),
            Some(2.0),
            Some(4.0),
            Some(6.0),
            Some(9.0)
        ]
    );
}

#[test]
fn cumsum_over_i64_column_promotes_to_f64() {
    let f = frame(vec![
        str_col("sym", &[(0, "A"), (1, "A"), (2, "A")]),
        i64_col("qty", &[(0, 2), (1, 3), (2, 5)]),
    ]);
    let got = cumsum(&f, "qty", &["sym"], None).unwrap();
    assert_eq!(got.data_type(), subms_ts::TsDataType::F64);
    assert_eq!(f64s(&got), vec![Some(2.0), Some(5.0), Some(10.0)]);
}

#[test]
fn cumprod_per_partition_matches_reference() {
    let f = frame(vec![
        f64_col("key", &[(0, 1.0), (1, 1.0), (2, 1.0)]),
        f64_col("val", &[(0, 2.0), (1, 3.0), (2, 4.0)]),
    ]);
    let got = cumprod(&f, "val", &["key"], None).unwrap();
    assert_eq!(f64s(&got), vec![Some(2.0), Some(6.0), Some(24.0)]);
}

#[test]
fn cummin_cummax_per_partition() {
    let f = frame(vec![
        f64_col("key", &[(0, 1.0), (1, 1.0), (2, 1.0), (3, 1.0)]),
        f64_col("val", &[(0, 5.0), (1, 3.0), (2, 8.0), (3, 1.0)]),
    ]);
    let mins = cummin(&f, "val", &["key"], None).unwrap();
    let maxs = cummax(&f, "val", &["key"], None).unwrap();
    assert_eq!(
        f64s(&mins),
        vec![Some(5.0), Some(3.0), Some(3.0), Some(1.0)]
    );
    assert_eq!(
        f64s(&maxs),
        vec![Some(5.0), Some(5.0), Some(8.0), Some(8.0)]
    );
}

#[test]
fn over_broadcasts_partition_aggregate_string_keyed() {
    let f = two_symbol_frame();
    let got = over(&f, &TsExpr::col("px").sum(), &["sym"]).unwrap();
    // AAPL px = 0+2+4 = 6 ; MSFT px = 1+3+5 = 9, broadcast across each row.
    assert_eq!(
        f64s(&got),
        vec![
            Some(6.0),
            Some(9.0),
            Some(6.0),
            Some(9.0),
            Some(6.0),
            Some(9.0)
        ]
    );
}

#[test]
fn over_count_yields_i64() {
    let f = two_symbol_frame();
    let got = over(&f, &TsExpr::col("px").count(), &["sym"]).unwrap();
    // each symbol has 3 rows.
    assert_eq!(got.data_type(), subms_ts::TsDataType::I64);
    assert_eq!(
        i64s(&got),
        vec![Some(3), Some(3), Some(3), Some(3), Some(3), Some(3)]
    );
}

#[test]
fn over_rejects_non_aggregation() {
    let f = two_symbol_frame();
    assert!(over(&f, &TsExpr::col("px"), &["sym"]).is_err());
}

#[test]
fn single_partition_behaves_as_global() {
    let f = frame(vec![
        f64_col("key", &[(0, 7.0), (1, 7.0), (2, 7.0)]),
        f64_col("val", &[(0, 1.0), (1, 2.0), (2, 3.0)]),
    ]);
    let cs = cumsum(&f, "val", &["key"], None).unwrap();
    assert_eq!(f64s(&cs), vec![Some(1.0), Some(3.0), Some(6.0)]);
    let rn = row_number(&f, &["key"], None).unwrap();
    assert_eq!(i64s(&rn), vec![Some(1), Some(2), Some(3)]);
}

#[test]
fn empty_frame_yields_empty_arrays() {
    let f = frame(vec![
        ("key".to_string(), TsColumn::F64(TsSeries::<f64>::new())),
        ("val".to_string(), TsColumn::F64(TsSeries::<f64>::new())),
    ]);
    assert_eq!(lag(&f, "val", 1, &["key"]).unwrap().len(), 0);
    assert_eq!(cumsum(&f, "val", &["key"], None).unwrap().len(), 0);
    assert_eq!(row_number(&f, &["key"], None).unwrap().len(), 0);
    assert_eq!(
        over(&f, &TsExpr::col("val").sum(), &["key"]).unwrap().len(),
        0
    );
}

#[test]
fn order_by_changes_the_result() {
    // Default order is arrival (ts) order; explicit order_by ascending reverses
    // the running scan relative to the ts axis.
    let f = frame(vec![
        f64_col("key", &[(0, 1.0), (1, 1.0), (2, 1.0)]),
        f64_col("val", &[(0, 1.0), (1, 2.0), (2, 4.0)]),
        f64_col("ord", &[(0, 30.0), (1, 20.0), (2, 10.0)]),
    ]);
    let default = cumsum(&f, "val", &["key"], None).unwrap();
    // default ts order: vals 1,2,4 -> 1,3,7
    assert_eq!(f64s(&default), vec![Some(1.0), Some(3.0), Some(7.0)]);

    let ordered = cumsum(&f, "val", &["key"], Some("ord")).unwrap();
    // ord ascending: ts2(10,val4), ts1(20,val2), ts0(30,val1).
    // running 4,6,7 scattered back to rows 2,1,0.
    assert_eq!(ordered.get(2), Some(TsValue::F64(4.0)));
    assert_eq!(ordered.get(1), Some(TsValue::F64(6.0)));
    assert_eq!(ordered.get(0), Some(TsValue::F64(7.0)));
}

#[test]
fn deterministic_across_runs() {
    let f = two_symbol_frame();
    let a = cumsum(&f, "px", &["sym"], None).unwrap();
    let b = cumsum(&f, "px", &["sym"], None).unwrap();
    assert_eq!(a, b);
}

#[test]
fn unknown_column_errors() {
    let f = two_symbol_frame();
    assert!(lag(&f, "nope", 1, &["sym"]).is_err());
    assert!(cumsum(&f, "px", &["missingkey"], None).is_err());
    assert!(rank(&f, &["sym"], "nope").is_err());
}

#[test]
fn cumsum_rejects_non_numeric_column() {
    let f = two_symbol_frame();
    // sym is a Str column; a running sum over it is a type error.
    assert_eq!(
        cumsum(&f, "sym", &["sym"], None),
        Err(subms_ts_window::TsWindowError::NotNumeric(
            "sym".to_string()
        ))
    );
}

#[test]
fn cumsum_carries_state_across_null_input() {
    // val has a hole at ts=1 on the aligned axis. The running sum carries the
    // previous total forward; the hole's output stays a valid running total.
    let f = frame(vec![
        f64_col("key", &[(0, 1.0), (1, 1.0), (2, 1.0)]),
        f64_col("val", &[(0, 10.0), (2, 5.0)]), // no val at ts=1
    ]);
    let got = cumsum(&f, "val", &["key"], None).unwrap();
    // row0: 10 ; row1: input null, acc stays 10 -> valid 10 ; row2: 10+5=15
    assert_eq!(f64s(&got), vec![Some(10.0), Some(10.0), Some(15.0)]);
}

#[test]
fn multi_typed_key_partitioning() {
    // two key columns of DIFFERENT types: a Str venue + an I64 side. Partition
    // is the typed tuple (venue, side).
    let f = frame(vec![
        str_col("venue", &[(0, "X"), (1, "X"), (2, "X"), (3, "X")]),
        i64_col("side", &[(0, 1), (1, 1), (2, 2), (3, 2)]),
        f64_col("val", &[(0, 1.0), (1, 2.0), (2, 3.0), (3, 4.0)]),
    ]);
    let got = cumsum(&f, "val", &["venue", "side"], None).unwrap();
    // (X,1): rows 0,1 vals 1,2 -> 1,3 ; (X,2): rows 2,3 vals 3,4 -> 3,7
    assert_eq!(f64s(&got), vec![Some(1.0), Some(3.0), Some(3.0), Some(7.0)]);
}
