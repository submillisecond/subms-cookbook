use subms_ts::TsSeries;
use subms_ts_asof_join::{asof_join_backward, asof_join_forward, asof_join_nearest};

fn series(pts: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(t, v) in pts {
        s.push(t, v).unwrap();
    }
    s
}

#[test]
fn backward_basic() {
    let l = series(&[(10, 1.0), (25, 2.0), (40, 3.0)]);
    let r = series(&[(5, 99.0), (20, 98.0), (30, 97.0)]);
    let m = asof_join_backward(&l, &r);
    assert_eq!(m.len(), 3);
    assert_eq!(m[0].right.map(|p| p.ts), Some(5)); // <=10
    assert_eq!(m[1].right.map(|p| p.ts), Some(20)); // <=25
    assert_eq!(m[2].right.map(|p| p.ts), Some(30)); // <=40
}

#[test]
fn backward_no_match_before_first() {
    let l = series(&[(1, 1.0), (10, 2.0)]);
    let r = series(&[(5, 99.0)]);
    let m = asof_join_backward(&l, &r);
    assert_eq!(m[0].right, None); // nothing <= 1
    assert_eq!(m[1].right.map(|p| p.ts), Some(5));
}

#[test]
fn forward_basic() {
    let l = series(&[(10, 1.0), (25, 2.0), (40, 3.0)]);
    let r = series(&[(5, 99.0), (20, 98.0), (30, 97.0)]);
    let m = asof_join_forward(&l, &r);
    assert_eq!(m[0].right.map(|p| p.ts), Some(20)); // >=10
    assert_eq!(m[1].right.map(|p| p.ts), Some(30)); // >=25
    assert_eq!(m[2].right, None); // nothing >= 40
}

#[test]
fn forward_exact_match() {
    let l = series(&[(20, 1.0)]);
    let r = series(&[(20, 99.0)]);
    assert_eq!(asof_join_forward(&l, &r)[0].right.map(|p| p.ts), Some(20));
    assert_eq!(asof_join_backward(&l, &r)[0].right.map(|p| p.ts), Some(20));
}

#[test]
fn nearest_within_tolerance() {
    let l = series(&[(10, 1.0), (100, 2.0)]);
    let r = series(&[(8, 99.0), (40, 98.0)]);
    // tol 5: trade@10 -> quote@8 (dist 2); trade@100 -> nearest is 40 (dist 60) > tol -> None
    let m = asof_join_nearest(&l, &r, 5);
    assert_eq!(m[0].right.map(|p| p.ts), Some(8));
    assert_eq!(m[1].right, None);
}

#[test]
fn nearest_picks_closer_side() {
    let l = series(&[(50, 1.0)]);
    let r = series(&[(40, 99.0), (58, 98.0)]);
    // back dist 10, fwd dist 8 -> pick fwd
    let m = asof_join_nearest(&l, &r, 100);
    assert_eq!(m[0].right.map(|p| p.ts), Some(58));
}

#[test]
fn nearest_tie_resolves_earlier() {
    let l = series(&[(50, 1.0)]);
    let r = series(&[(45, 99.0), (55, 98.0)]); // both dist 5
    let m = asof_join_nearest(&l, &r, 100);
    assert_eq!(m[0].right.map(|p| p.ts), Some(45));
}

#[test]
fn empty_right_all_none() {
    let l = series(&[(1, 1.0), (2, 2.0)]);
    let r = TsSeries::<f64>::new();
    for m in asof_join_backward(&l, &r) {
        assert_eq!(m.right, None);
    }
    assert_eq!(asof_join_forward(&l, &r).len(), 2);
}

#[test]
fn empty_left_empty_result() {
    let l = TsSeries::<f64>::new();
    let r = series(&[(1, 1.0)]);
    assert!(asof_join_backward(&l, &r).is_empty());
}

#[test]
fn left_values_preserved() {
    let l = series(&[(10, 1.5), (20, 2.5)]);
    let r = series(&[(5, 9.0)]);
    let m = asof_join_backward(&l, &r);
    assert_eq!(m[0].left.value, 1.5);
    assert_eq!(m[1].left.value, 2.5);
}

#[test]
fn dense_merge_walk_correct() {
    // many points, cross-check backward against a brute-force scan
    let l = series(&(0..1_000).map(|i| (i * 3, i as f64)).collect::<Vec<_>>());
    let r = series(&(0..1_000).map(|i| (i * 2, i as f64)).collect::<Vec<_>>());
    let m = asof_join_backward(&l, &r);
    let rv: Vec<(i64, f64)> = r.iter().map(|p| (p.ts, p.value)).collect();
    for row in &m {
        let expect = rv.iter().rfind(|(t, _)| *t <= row.left.ts).map(|&(t, _)| t);
        assert_eq!(row.right.map(|p| p.ts), expect, "left ts {}", row.left.ts);
    }
}
