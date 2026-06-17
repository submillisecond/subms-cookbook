use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_reshape::{
    PivotAgg, TsArray, TsReshapeError, except, explode, frame_columns, frame_value_cells, hstack,
    intersect, melt, pivot, union, vstack,
};

// ---------- frame builders ----------

// Each builder pushes row i at ts=i so the flattened row axis is the dense 0..n
// range and every column is fully present (unless a builder leaves a gap).

fn str_col(vals: &[&str]) -> TsColumn {
    let mut s = TsSeries::<String>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, v.to_string()).unwrap();
    }
    TsColumn::Str(s)
}

fn i64_col(vals: &[i64]) -> TsColumn {
    let mut s = TsSeries::<i64>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, *v).unwrap();
    }
    TsColumn::I64(s)
}

fn f64_col(vals: &[f64]) -> TsColumn {
    let mut s = TsSeries::<f64>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, *v).unwrap();
    }
    TsColumn::F64(s)
}

fn bool_col(vals: &[bool]) -> TsColumn {
    let mut s = TsSeries::<bool>::new();
    for (i, v) in vals.iter().enumerate() {
        s.push(i as i64, *v).unwrap();
    }
    TsColumn::Bool(s)
}

fn value_col(vals: Vec<TsValue>) -> TsColumn {
    let mut s = TsSeries::<TsValue>::new();
    for (i, v) in vals.into_iter().enumerate() {
        s.push(i as i64, v).unwrap();
    }
    TsColumn::Value(s)
}

fn col_f64(arr: &TsArray) -> Vec<Option<f64>> {
    (0..arr.len())
        .map(|i| match arr.get(i) {
            Some(TsValue::F64(v)) => Some(v),
            Some(TsValue::I64(v)) => Some(v as f64),
            _ => None,
        })
        .collect()
}

fn col_str(arr: &TsArray) -> Vec<Option<String>> {
    (0..arr.len())
        .map(|i| match arr.get(i) {
            Some(TsValue::Str(s)) => Some(s),
            _ => None,
        })
        .collect()
}

// A long (idx, cat-as-STRING, reading) fixture. (idx=1, cat="s2") is absent on
// purpose so we can assert a missing pivot cell.
fn long_fixture() -> TsDataFrame {
    TsDataFrame::new()
        .with_column("idx", i64_col(&[0, 0, 1, 1, 0]))
        .with_column("cat", str_col(&["s1", "s2", "s1", "s1", "s1"]))
        .with_column("reading", f64_col(&[10.0, 20.0, 11.0, 13.0, 30.0]))
}

// ---------- pivot ----------

#[test]
fn pivot_long_to_wide_on_string_category_matches_reference() {
    let out = pivot(&long_fixture(), "idx", "cat", "reading", PivotAgg::Sum).unwrap();
    // index axis: distinct idx in first-seen order -> [0, 1].
    // category axis: distinct cat sorted -> ["s1", "s2"], named by the string.
    assert_eq!(out.nrows(), 2);
    assert_eq!(out.ncols(), 3); // idx + s1 + s2
    let names: Vec<&str> = out.column_names().collect();
    assert_eq!(names, vec!["idx", "s1", "s2"]);

    // idx column keeps its i64 type and values.
    assert_eq!(col_f64(out.column("idx").unwrap()), vec![Some(0.0), Some(1.0)]);

    // s1: idx0 -> 10+30 = 40 ; idx1 -> 11+13 = 24.
    assert_eq!(col_f64(out.column("s1").unwrap()), vec![Some(40.0), Some(24.0)]);
    // s2: idx0 -> 20 ; idx1 -> ABSENT (null), the hand-rolled missing combo.
    assert_eq!(col_f64(out.column("s2").unwrap()), vec![Some(20.0), None]);
}

#[test]
fn pivot_each_agg() {
    let f = long_fixture();
    let mean = pivot(&f, "idx", "cat", "reading", PivotAgg::Mean).unwrap();
    // s1 idx0 mean of [10, 30] = 20.
    assert_eq!(col_f64(mean.column("s1").unwrap())[0], Some(20.0));

    let min = pivot(&f, "idx", "cat", "reading", PivotAgg::Min).unwrap();
    assert_eq!(col_f64(min.column("s1").unwrap())[0], Some(10.0));

    let max = pivot(&f, "idx", "cat", "reading", PivotAgg::Max).unwrap();
    assert_eq!(col_f64(max.column("s1").unwrap())[0], Some(30.0));

    let last = pivot(&f, "idx", "cat", "reading", PivotAgg::Last).unwrap();
    // idx0 s1 rows in input order: 10 (row0), 30 (row4) -> last is 30.
    assert_eq!(col_f64(last.column("s1").unwrap())[0], Some(30.0));

    let sum = pivot(&f, "idx", "cat", "reading", PivotAgg::Sum).unwrap();
    assert_eq!(col_f64(sum.column("s1").unwrap())[0], Some(40.0));
}

#[test]
fn pivot_unknown_column_errors() {
    let f = long_fixture();
    let err = pivot(&f, "idx", "nope", "reading", PivotAgg::Sum).unwrap_err();
    assert_eq!(
        err,
        TsReshapeError::UnknownColumn {
            name: "nope".to_string()
        }
    );
}

// ---------- melt (headline: the Str variable column) ----------

#[test]
fn melt_wide_to_long_carries_str_variable_and_value_cells() {
    let f = TsDataFrame::new()
        .with_column("day", i64_col(&[0, 1]))
        .with_column("open", f64_col(&[10.0, 20.0]))
        .with_column("close", f64_col(&[11.0, 22.0]));

    let out = melt(&f, &["day"], &["open", "close"]).unwrap();
    // 2 input rows x 2 value cols = 4 long rows.
    assert_eq!(out.nrows(), 4);
    let names: Vec<&str> = out.column_names().collect();
    assert_eq!(names, vec!["day", "variable", "value"]);

    // the variable column is a REAL Str column naming the source slots.
    let var = out.column("variable").unwrap();
    assert_eq!(var.data_type(), subms_ts::TsDataType::Str);
    assert_eq!(
        col_str(var),
        vec![
            Some("open".to_string()),
            Some("close".to_string()),
            Some("open".to_string()),
            Some("close".to_string()),
        ]
    );

    // the id column repeats per value column.
    assert_eq!(
        col_f64(out.column("day").unwrap()),
        vec![Some(0.0), Some(0.0), Some(1.0), Some(1.0)]
    );

    // value cells follow (row, value_col) order; both value cols are f64 so the
    // shared dtype is f64.
    let value = out.column("value").unwrap();
    assert_eq!(value.data_type(), subms_ts::TsDataType::F64);
    assert_eq!(
        col_f64(value),
        vec![Some(10.0), Some(11.0), Some(20.0), Some(22.0)]
    );
}

#[test]
fn melt_mixed_type_value_cols_collapse_to_str_value_column() {
    let f = TsDataFrame::new()
        .with_column("id", i64_col(&[0, 1]))
        .with_column("name", str_col(&["aa", "bb"]))
        .with_column("score", f64_col(&[1.5, 2.0]));

    // name is Str, score is F64 -> mixed -> value column is Str.
    let out = melt(&f, &["id"], &["name", "score"]).unwrap();
    let value = out.column("value").unwrap();
    assert_eq!(value.data_type(), subms_ts::TsDataType::Str);
    assert_eq!(
        col_str(value),
        vec![
            Some("aa".to_string()),
            Some("1.5".to_string()),
            Some("bb".to_string()),
            Some("2".to_string()),
        ]
    );
    // variable still names the columns.
    assert_eq!(
        col_str(out.column("variable").unwrap()),
        vec![
            Some("name".to_string()),
            Some("score".to_string()),
            Some("name".to_string()),
            Some("score".to_string()),
        ]
    );
}

#[test]
fn melt_no_value_cols_errors() {
    let f = TsDataFrame::new().with_column("id", i64_col(&[0, 1]));
    assert_eq!(melt(&f, &["id"], &[]).unwrap_err(), TsReshapeError::NoColumns);
}

#[test]
fn melt_unknown_value_col_errors() {
    let f = TsDataFrame::new()
        .with_column("id", i64_col(&[0]))
        .with_column("v", f64_col(&[1.0]));
    let err = melt(&f, &["id"], &["v", "missing"]).unwrap_err();
    assert_eq!(
        err,
        TsReshapeError::UnknownColumn {
            name: "missing".to_string()
        }
    );
}

// ---------- explode ----------

#[test]
fn explode_value_array_emits_one_row_per_element() {
    let f = TsDataFrame::new()
        .with_column("id", i64_col(&[0, 1]))
        .with_column(
            "tags",
            value_col(vec![
                TsValue::Array(vec![TsValue::F64(1.0), TsValue::F64(2.0)]),
                TsValue::Array(vec![TsValue::F64(3.0)]),
            ]),
        );

    let out = explode(&f, "tags").unwrap();
    // row0 has 2 elems, row1 has 1 -> 3 output rows.
    assert_eq!(out.nrows(), 3);
    // id repeats per element.
    assert_eq!(
        col_f64(out.column("id").unwrap()),
        vec![Some(0.0), Some(0.0), Some(1.0)]
    );
    // exploded tags are flattened numerics.
    assert_eq!(
        col_f64(out.column("tags").unwrap()),
        vec![Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn explode_empty_array_drops_the_row() {
    let f = TsDataFrame::new()
        .with_column("id", i64_col(&[0, 1, 2]))
        .with_column(
            "tags",
            value_col(vec![
                TsValue::Array(vec![TsValue::F64(1.0)]),
                TsValue::Array(vec![]), // empty -> dropped
                TsValue::Array(vec![TsValue::F64(9.0), TsValue::F64(8.0)]),
            ]),
        );

    let out = explode(&f, "tags").unwrap();
    // row1's empty list contributes nothing: 1 + 0 + 2 = 3 rows.
    assert_eq!(out.nrows(), 3);
    assert_eq!(
        col_f64(out.column("id").unwrap()),
        vec![Some(0.0), Some(2.0), Some(2.0)]
    );
    assert_eq!(
        col_f64(out.column("tags").unwrap()),
        vec![Some(1.0), Some(9.0), Some(8.0)]
    );
}

#[test]
fn explode_string_elements_emit_str_column() {
    let f = TsDataFrame::new()
        .with_column("id", i64_col(&[0]))
        .with_column(
            "labels",
            value_col(vec![TsValue::Array(vec![
                TsValue::Str("x".into()),
                TsValue::Str("y".into()),
            ])]),
        );
    let out = explode(&f, "labels").unwrap();
    assert_eq!(out.nrows(), 2);
    assert_eq!(
        col_str(out.column("labels").unwrap()),
        vec![Some("x".to_string()), Some("y".to_string())]
    );
}

#[test]
fn explode_unknown_column_errors() {
    let f = TsDataFrame::new().with_column("id", i64_col(&[0]));
    assert_eq!(
        explode(&f, "nope").unwrap_err(),
        TsReshapeError::UnknownColumn {
            name: "nope".to_string()
        }
    );
}

// ---------- vstack / hstack ----------

#[test]
fn vstack_same_schema_concatenates_rows() {
    let a = TsDataFrame::new()
        .with_column("k", str_col(&["a", "b"]))
        .with_column("v", f64_col(&[1.0, 2.0]));
    let b = TsDataFrame::new()
        .with_column("k", str_col(&["c"]))
        .with_column("v", f64_col(&[3.0]));
    let out = vstack(&a, &b).unwrap();
    assert_eq!(out.nrows(), 3);
    assert_eq!(
        col_str(out.column("k").unwrap()),
        vec![Some("a".to_string()), Some("b".to_string()), Some("c".to_string())]
    );
    assert_eq!(
        col_f64(out.column("v").unwrap()),
        vec![Some(1.0), Some(2.0), Some(3.0)]
    );
}

#[test]
fn vstack_schema_mismatch_errors() {
    let a = TsDataFrame::new().with_column("k", f64_col(&[1.0]));
    let b = TsDataFrame::new().with_column("other", f64_col(&[1.0]));
    let err = vstack(&a, &b).unwrap_err();
    assert_eq!(
        err,
        TsReshapeError::SchemaMismatch {
            a: vec!["k".to_string()],
            b: vec!["other".to_string()],
        }
    );
}

#[test]
fn hstack_with_collision_suffixes() {
    let a = TsDataFrame::new()
        .with_column("id", i64_col(&[0, 1]))
        .with_column("v", f64_col(&[1.0, 2.0]));
    let b = TsDataFrame::new()
        .with_column("v", f64_col(&[3.0, 4.0]))
        .with_column("w", f64_col(&[5.0, 6.0]));
    let out = hstack(&a, &b).unwrap();
    let names: Vec<&str> = out.column_names().collect();
    // "v" collides -> v_a / v_b ; "id" and "w" unique.
    assert_eq!(names, vec!["id", "v_a", "v_b", "w"]);
    assert_eq!(out.nrows(), 2);
    assert_eq!(col_f64(out.column("v_a").unwrap()), vec![Some(1.0), Some(2.0)]);
    assert_eq!(col_f64(out.column("v_b").unwrap()), vec![Some(3.0), Some(4.0)]);
}

#[test]
fn hstack_row_count_mismatch_errors() {
    let a = TsDataFrame::new().with_column("a", f64_col(&[1.0, 2.0]));
    let b = TsDataFrame::new().with_column("b", f64_col(&[3.0]));
    let err = hstack(&a, &b).unwrap_err();
    assert_eq!(err, TsReshapeError::RowCountMismatch { a: 2, b: 1 });
}

// ---------- row set-ops ----------

fn pair() -> (TsDataFrame, TsDataFrame) {
    // a rows: (x,1),(y,2),(x,1 dup),(z,3) ; b rows: (y,2),(z,3),(w,4).
    let a = TsDataFrame::new()
        .with_column("k", str_col(&["x", "y", "x", "z"]))
        .with_column("v", i64_col(&[1, 2, 1, 3]));
    let b = TsDataFrame::new()
        .with_column("k", str_col(&["y", "z", "w"]))
        .with_column("v", i64_col(&[2, 3, 4]));
    (a, b)
}

#[test]
fn union_distinct_rows_in_deterministic_order() {
    let (a, b) = pair();
    let out = union(&a, &b).unwrap();
    // distinct a: (x,1),(y,2),(z,3) then b-only: (w,4) -> 4 rows.
    assert_eq!(out.nrows(), 4);
    assert_eq!(
        col_str(out.column("k").unwrap()),
        vec![
            Some("x".to_string()),
            Some("y".to_string()),
            Some("z".to_string()),
            Some("w".to_string()),
        ]
    );
    assert_eq!(
        col_f64(out.column("v").unwrap()),
        vec![Some(1.0), Some(2.0), Some(3.0), Some(4.0)]
    );
}

#[test]
fn intersect_distinct_common_rows() {
    let (a, b) = pair();
    let out = intersect(&a, &b).unwrap();
    // common: (y,2),(z,3) in a-order.
    assert_eq!(out.nrows(), 2);
    assert_eq!(
        col_str(out.column("k").unwrap()),
        vec![Some("y".to_string()), Some("z".to_string())]
    );
}

#[test]
fn except_distinct_a_minus_b() {
    let (a, b) = pair();
    let out = except(&a, &b).unwrap();
    // a-only distinct: (x,1) (the dup collapses).
    assert_eq!(out.nrows(), 1);
    assert_eq!(col_str(out.column("k").unwrap()), vec![Some("x".to_string())]);
    assert_eq!(col_f64(out.column("v").unwrap()), vec![Some(1.0)]);
}

#[test]
fn set_op_typed_cells_str_3_distinct_from_numeric_3() {
    // Str "3" and I64 3 sit in different columns, but the typed key ensures a
    // row (k="3") never equals a row (k=3) under a schema with one Str column.
    let a = TsDataFrame::new().with_column("k", str_col(&["3"]));
    let b = TsDataFrame::new().with_column("k", str_col(&["4"]));
    let out = union(&a, &b).unwrap();
    assert_eq!(out.nrows(), 2);
    assert_eq!(
        col_str(out.column("k").unwrap()),
        vec![Some("3".to_string()), Some("4".to_string())]
    );
}

#[test]
fn set_op_schema_mismatch_errors() {
    let a = TsDataFrame::new().with_column("k", f64_col(&[1.0]));
    let b = TsDataFrame::new().with_column("j", f64_col(&[1.0]));
    assert!(matches!(
        union(&a, &b).unwrap_err(),
        TsReshapeError::SchemaMismatch { .. }
    ));
    assert!(matches!(
        intersect(&a, &b).unwrap_err(),
        TsReshapeError::SchemaMismatch { .. }
    ));
    assert!(matches!(
        except(&a, &b).unwrap_err(),
        TsReshapeError::SchemaMismatch { .. }
    ));
}

// ---------- empty frames + flattening helpers ----------

#[test]
fn empty_frame_reshapes_to_empty() {
    let empty = TsDataFrame::new();
    // pivot on an empty frame fails on the unknown column (no columns at all).
    assert!(pivot(&empty, "i", "c", "v", PivotAgg::Sum).is_err());

    // vstack of two empty frames is empty (matching schema = no columns).
    let out = vstack(&empty, &TsDataFrame::new()).unwrap();
    assert!(out.is_empty());
    assert_eq!(out.ncols(), 0);
}

#[test]
fn pivot_empty_after_all_rows_skipped() {
    // every value cell is a Bool, which cannot reduce to f64 -> all rows skipped.
    let f = TsDataFrame::new()
        .with_column("idx", i64_col(&[0, 1]))
        .with_column("cat", str_col(&["a", "b"]))
        .with_column("val", bool_col(&[true, false]));
    let out = pivot(&f, "idx", "cat", "val", PivotAgg::Sum).unwrap();
    assert!(out.is_empty());
}

#[test]
fn frame_columns_exposes_typed_flattening() {
    let f = long_fixture();
    let cols = frame_columns(&f);
    assert_eq!(cols.len(), 3);
    assert_eq!(cols[0].0, "idx");
    assert_eq!(cols[0].1.data_type(), subms_ts::TsDataType::I64);
    assert_eq!(cols[1].1.data_type(), subms_ts::TsDataType::Str);
}

#[test]
fn frame_value_cells_exposes_value_columns() {
    let f = TsDataFrame::new()
        .with_column("id", i64_col(&[0]))
        .with_column(
            "doc",
            value_col(vec![TsValue::Array(vec![TsValue::F64(1.0)])]),
        );
    let vc = frame_value_cells(&f);
    assert!(vc.contains_key("doc"));
    assert!(!vc.contains_key("id"));
    assert_eq!(vc["doc"].len(), 1);
}

#[test]
fn result_column_at_and_names() {
    let f = long_fixture();
    let out = pivot(&f, "idx", "cat", "reading", PivotAgg::Sum).unwrap();
    assert!(out.column_at(0).is_some());
    assert!(out.column_at(99).is_none());
    assert!(out.column("nope").is_none());
    assert_eq!(out.columns().len(), out.ncols());
}
