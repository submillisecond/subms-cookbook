//! Correctness tests for the typed equi-join over [`TsDataFrame`]. std-only,
//! NOT gated on the harness feature. The headline test joins on a STRING key;
//! the rest sweep the kind matrix, null validity, multi-key, hash == sort-merge
//! agreement, and the edge cases.

use std::collections::HashSet;

use subms_ts::{TsColumn, TsDataFrame, TsSeries, TsValue};
use subms_ts_expr::TsArray;
use subms_ts_join::{
    TsJoinError, TsJoinKind, TsJoinResult, cross_join, frame_columns, hash_join, sort_merge_join,
};

// ---------- frame builders ----------

fn str_col(rows: &[(i64, &str)]) -> TsColumn {
    let mut s = TsSeries::<String>::new();
    for &(ts, v) in rows {
        s.push(ts, v.to_string()).unwrap();
    }
    TsColumn::Str(s)
}

fn i64_col(rows: &[(i64, i64)]) -> TsColumn {
    let mut s = TsSeries::<i64>::new();
    for &(ts, v) in rows {
        s.push(ts, v).unwrap();
    }
    TsColumn::I64(s)
}

fn f64_col(rows: &[(i64, f64)]) -> TsColumn {
    let mut s = TsSeries::<f64>::new();
    for &(ts, v) in rows {
        s.push(ts, v).unwrap();
    }
    TsColumn::F64(s)
}

// left frame: sym (Str) + px (F64), one row per ts so the aligned axis is dense.
fn quotes() -> TsDataFrame {
    TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL"), (1, "MSFT"), (2, "GOOG")]))
        .with_column("px", f64_col(&[(0, 10.0), (1, 20.0), (2, 30.0)]))
}

// right frame: sym (Str) + qty (F64). AAPL + GOOG match quotes; AMZN is right-only.
fn trades() -> TsDataFrame {
    TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL"), (1, "GOOG"), (2, "AMZN")]))
        .with_column("qty", f64_col(&[(0, 100.0), (1, 300.0), (2, 400.0)]))
}

fn cells(arr: &TsArray) -> Vec<Option<TsValue>> {
    (0..arr.len()).map(|i| arr.get(i)).collect()
}

fn str_at(res: &TsJoinResult, col: &str, row: usize) -> Option<String> {
    match res.column(col).and_then(|c| c.get(row)) {
        Some(TsValue::Str(s)) => Some(s),
        _ => None,
    }
}

fn f64_at(res: &TsJoinResult, col: &str, row: usize) -> Option<f64> {
    match res.column(col).and_then(|c| c.get(row)) {
        Some(TsValue::F64(v)) => Some(v),
        _ => None,
    }
}

// A set of joined rows as comparable strings, so hash and sort-merge results
// can be checked for set-equality regardless of row order.
fn row_set(res: &TsJoinResult) -> HashSet<String> {
    let names: Vec<String> = res.column_names().map(|s| s.to_string()).collect();
    (0..res.nrows())
        .map(|r| {
            names
                .iter()
                .map(|n| format!("{n}={:?}", res.column(n).unwrap().get(r)))
                .collect::<Vec<_>>()
                .join("|")
        })
        .collect()
}

// ---------- 1. headline: inner join on a STRING key ----------

#[test]
fn inner_join_on_string_key_matches_reference() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Inner).unwrap();

    // hand-rolled reference: AAPL and GOOG are the only shared symbols.
    assert_eq!(out.nrows(), 2);
    let syms: HashSet<String> = (0..out.nrows())
        .filter_map(|r| str_at(&out, "sym", r))
        .collect();
    assert_eq!(syms, HashSet::from(["AAPL".to_string(), "GOOG".to_string()]));

    for r in 0..out.nrows() {
        match str_at(&out, "sym", r).as_deref() {
            Some("AAPL") => {
                assert_eq!(f64_at(&out, "px", r), Some(10.0));
                assert_eq!(f64_at(&out, "qty", r), Some(100.0));
            }
            Some("GOOG") => {
                assert_eq!(f64_at(&out, "px", r), Some(30.0));
                assert_eq!(f64_at(&out, "qty", r), Some(300.0));
            }
            other => panic!("unexpected joined symbol {other:?}"),
        }
    }
}

// ---------- 2. left join fills unmatched-right cells NULL ----------

#[test]
fn left_join_nulls_unmatched_right() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Left).unwrap();
    assert_eq!(out.nrows(), 3);
    let msft = (0..out.nrows())
        .find(|&r| str_at(&out, "sym", r).as_deref() == Some("MSFT"))
        .unwrap();
    assert_eq!(f64_at(&out, "px", msft), Some(20.0));
    assert_eq!(out.column("qty").unwrap().get(msft), None);
    assert!(!out.column("qty").unwrap().valid()[msft]);
}

// ---------- 3. right join ----------

#[test]
fn right_join_nulls_unmatched_left() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Right).unwrap();
    assert_eq!(out.nrows(), 3);
    let amzn = (0..out.nrows())
        .find(|&r| str_at(&out, "sym", r).as_deref() == Some("AMZN"))
        .unwrap();
    assert_eq!(out.column("px").unwrap().get(amzn), None);
    assert_eq!(f64_at(&out, "qty", amzn), Some(400.0));
}

// ---------- 4. outer join ----------

#[test]
fn outer_join_keeps_both_unmatched_sides() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Outer).unwrap();
    assert_eq!(out.nrows(), 4);
    let syms: HashSet<String> = (0..out.nrows())
        .filter_map(|r| str_at(&out, "sym", r))
        .collect();
    assert_eq!(
        syms,
        HashSet::from([
            "AAPL".to_string(),
            "MSFT".to_string(),
            "GOOG".to_string(),
            "AMZN".to_string(),
        ])
    );
}

// ---------- 5. semi join ----------

#[test]
fn semi_join_emits_matching_left_rows_only() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Semi).unwrap();
    assert_eq!(out.nrows(), 2);
    let names: Vec<String> = out.column_names().map(|s| s.to_string()).collect();
    assert!(names.contains(&"sym".to_string()));
    assert!(names.contains(&"px".to_string()));
    assert!(!names.contains(&"qty".to_string()));
}

// ---------- 6. anti join ----------

#[test]
fn anti_join_emits_unmatched_left_rows_only() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Anti).unwrap();
    assert_eq!(out.nrows(), 1);
    assert_eq!(str_at(&out, "sym", 0).as_deref(), Some("MSFT"));
    let names: Vec<String> = out.column_names().map(|s| s.to_string()).collect();
    assert!(!names.contains(&"qty".to_string()));
}

// ---------- 7. cross join ----------

#[test]
fn cross_join_is_cartesian_product() {
    let out = cross_join(&quotes(), &trades());
    assert_eq!(out.nrows(), 9);
    let names: Vec<String> = out.column_names().map(|s| s.to_string()).collect();
    assert!(names.contains(&"sym_left".to_string()));
    assert!(names.contains(&"sym_right".to_string()));
    for r in 0..out.nrows() {
        assert!(out.column("sym_left").unwrap().get(r).is_some());
        assert!(out.column("sym_right").unwrap().get(r).is_some());
    }
}

// ---------- 8. multi-key join: STRING + INT ----------

#[test]
fn multi_key_join_on_string_and_int() {
    let left = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL"), (1, "AAPL"), (2, "MSFT")]))
        .with_column("day", i64_col(&[(0, 1), (1, 2), (2, 1)]))
        .with_column("px", f64_col(&[(0, 10.0), (1, 11.0), (2, 20.0)]));
    let right = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL"), (1, "MSFT")]))
        .with_column("day", i64_col(&[(0, 1), (1, 1)]))
        .with_column("qty", f64_col(&[(0, 100.0), (1, 200.0)]));

    let out =
        hash_join(&left, &right, &["sym", "day"], &["sym", "day"], TsJoinKind::Inner).unwrap();
    assert_eq!(out.nrows(), 2);
    for r in 0..out.nrows() {
        let sym = str_at(&out, "sym", r).unwrap();
        let day = match out.column("day").unwrap().get(r) {
            Some(TsValue::I64(d)) => d,
            other => panic!("day not i64: {other:?}"),
        };
        assert_eq!(day, 1);
        match sym.as_str() {
            "AAPL" => assert_eq!(f64_at(&out, "qty", r), Some(100.0)),
            "MSFT" => assert_eq!(f64_at(&out, "qty", r), Some(200.0)),
            o => panic!("unexpected {o}"),
        }
    }
}

// ---------- 9. sort-merge == hash result SET across all kinds ----------

#[test]
fn sort_merge_matches_hash_for_every_kind() {
    let kinds = [
        TsJoinKind::Inner,
        TsJoinKind::Left,
        TsJoinKind::Right,
        TsJoinKind::Outer,
        TsJoinKind::Semi,
        TsJoinKind::Anti,
    ];
    for k in kinds {
        let h = hash_join(&quotes(), &trades(), &["sym"], &["sym"], k).unwrap();
        let m = sort_merge_join(&quotes(), &trades(), &["sym"], &["sym"], k).unwrap();
        assert_eq!(h.nrows(), m.nrows(), "{k:?} nrows differ");
        assert_eq!(row_set(&h), row_set(&m), "{k:?} row sets differ");
    }
}

// ---------- 10. empty side ----------

#[test]
fn empty_right_inner_is_empty_left_keeps_all() {
    let empty = TsDataFrame::new()
        .with_column("sym", str_col(&[]))
        .with_column("qty", f64_col(&[]));
    let inner = hash_join(&quotes(), &empty, &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
    assert!(inner.is_empty());
    let left = hash_join(&quotes(), &empty, &["sym"], &["sym"], TsJoinKind::Left).unwrap();
    assert_eq!(left.nrows(), 3);
    for r in 0..left.nrows() {
        assert_eq!(left.column("qty").unwrap().get(r), None);
    }
}

// ---------- 11. no matches at all ----------

#[test]
fn disjoint_keys_produce_no_inner_rows() {
    let other = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "TSLA"), (1, "NVDA")]))
        .with_column("qty", f64_col(&[(0, 1.0), (1, 2.0)]));
    let inner = hash_join(&quotes(), &other, &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
    assert!(inner.is_empty());
    let outer = hash_join(&quotes(), &other, &["sym"], &["sym"], TsJoinKind::Outer).unwrap();
    assert_eq!(outer.nrows(), 5);
}

// ---------- 12. duplicate keys: one-to-many ----------

#[test]
fn duplicate_keys_fan_out_one_to_many() {
    let left = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL")]))
        .with_column("px", f64_col(&[(0, 10.0)]));
    let right = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL"), (1, "AAPL"), (2, "AAPL")]))
        .with_column("qty", f64_col(&[(0, 1.0), (1, 2.0), (2, 3.0)]));
    let out = hash_join(&left, &right, &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
    assert_eq!(out.nrows(), 3);
    let qtys: Vec<f64> = (0..out.nrows())
        .filter_map(|r| f64_at(&out, "qty", r))
        .collect();
    assert_eq!(qtys, vec![1.0, 2.0, 3.0]);
}

// ---------- 13. collision suffixing on payload columns ----------

#[test]
fn payload_name_collision_is_suffixed() {
    let left = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL")]))
        .with_column("vol", f64_col(&[(0, 1.0)]));
    let right = TsDataFrame::new()
        .with_column("sym", str_col(&[(0, "AAPL")]))
        .with_column("vol", f64_col(&[(0, 2.0)]));
    let out = hash_join(&left, &right, &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
    let names: Vec<String> = out.column_names().map(|s| s.to_string()).collect();
    assert!(names.contains(&"vol_left".to_string()));
    assert!(names.contains(&"vol_right".to_string()));
    assert_eq!(f64_at(&out, "vol_left", 0), Some(1.0));
    assert_eq!(f64_at(&out, "vol_right", 0), Some(2.0));
}

// ---------- 14. deterministic output order (left-driving) ----------

#[test]
fn hash_join_output_is_left_driving_deterministic() {
    let out = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
    let order: Vec<String> = (0..out.nrows())
        .filter_map(|r| str_at(&out, "sym", r))
        .collect();
    assert_eq!(order, vec!["AAPL".to_string(), "GOOG".to_string()]);
    let again = hash_join(&quotes(), &trades(), &["sym"], &["sym"], TsJoinKind::Inner).unwrap();
    let order2: Vec<String> = (0..again.nrows())
        .filter_map(|r| str_at(&again, "sym", r))
        .collect();
    assert_eq!(order, order2);
}

// ---------- 15. error surface ----------

#[test]
fn unknown_key_and_arity_and_no_keys_error() {
    let q = quotes();
    let t = trades();
    assert_eq!(
        hash_join(&q, &t, &["nope"], &["sym"], TsJoinKind::Inner).unwrap_err(),
        TsJoinError::UnknownKey {
            side: "left",
            name: "nope".to_string()
        }
    );
    assert_eq!(
        hash_join(&q, &t, &["sym"], &["sym", "qty"], TsJoinKind::Inner).unwrap_err(),
        TsJoinError::KeyArityMismatch { left: 1, right: 2 }
    );
    assert_eq!(
        hash_join(&q, &t, &[], &[], TsJoinKind::Inner).unwrap_err(),
        TsJoinError::NoKeys
    );
}

// ---------- 16. join on an INT key alone ----------

#[test]
fn int_key_join_works() {
    let left = TsDataFrame::new()
        .with_column("day", i64_col(&[(0, 1), (1, 2), (2, 3)]))
        .with_column("px", f64_col(&[(0, 10.0), (1, 20.0), (2, 30.0)]));
    let right = TsDataFrame::new()
        .with_column("day", i64_col(&[(0, 2), (1, 3), (2, 4)]))
        .with_column("qty", f64_col(&[(0, 200.0), (1, 300.0), (2, 400.0)]));
    let out = hash_join(&left, &right, &["day"], &["day"], TsJoinKind::Inner).unwrap();
    assert_eq!(out.nrows(), 2);
}

// ---------- 17. frame_columns flattening is exposed + typed ----------

#[test]
fn frame_columns_exposes_typed_dense_arrays() {
    let cols = frame_columns(&quotes());
    assert_eq!(cols.len(), 2);
    let sym = &cols.iter().find(|(n, _)| n == "sym").unwrap().1;
    assert!(matches!(sym, TsArray::Str { .. }));
    assert_eq!(cells(sym).len(), 3);
    let px = &cols.iter().find(|(n, _)| n == "px").unwrap().1;
    assert!(matches!(px, TsArray::F64 { .. }));
}

// ---------- 18. sort-merge over int keys equals hash ----------

#[test]
fn sort_merge_int_key_inner_equals_hash() {
    let left = TsDataFrame::new()
        .with_column("day", i64_col(&[(0, 3), (1, 1), (2, 2)]))
        .with_column("px", f64_col(&[(0, 30.0), (1, 10.0), (2, 20.0)]));
    let right = TsDataFrame::new()
        .with_column("day", i64_col(&[(0, 2), (1, 1)]))
        .with_column("qty", f64_col(&[(0, 200.0), (1, 100.0)]));
    let h = hash_join(&left, &right, &["day"], &["day"], TsJoinKind::Inner).unwrap();
    let m = sort_merge_join(&left, &right, &["day"], &["day"], TsJoinKind::Inner).unwrap();
    assert_eq!(row_set(&h), row_set(&m));
    assert_eq!(h.nrows(), 2);
}
