use subms_ts::TsSeries;
use subms_ts_fill::{fill_linear, fill_locf, fill_zero};

fn series(pts: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(t, v) in pts {
        s.push(t, v).unwrap();
    }
    s
}

fn pairs(s: &TsSeries<f64>) -> Vec<(i64, f64)> {
    s.iter().map(|p| (p.ts, p.value)).collect()
}

#[test]
fn linear_fills_gap() {
    let s = series(&[(0, 0.0), (40, 4.0)]);
    assert_eq!(
        pairs(&fill_linear(&s, 10)),
        vec![(0, 0.0), (10, 1.0), (20, 2.0), (30, 3.0), (40, 4.0)]
    );
}

#[test]
fn locf_carries_left() {
    let s = series(&[(0, 7.0), (30, 9.0)]);
    assert_eq!(
        pairs(&fill_locf(&s, 10)),
        vec![(0, 7.0), (10, 7.0), (20, 7.0), (30, 9.0)]
    );
}

#[test]
fn zero_fills_gap() {
    let s = series(&[(0, 5.0), (30, 6.0)]);
    assert_eq!(
        pairs(&fill_zero(&s, 10)),
        vec![(0, 5.0), (10, 0.0), (20, 0.0), (30, 6.0)]
    );
}

#[test]
fn no_gap_passthrough() {
    let s = series(&[(0, 1.0), (10, 2.0), (20, 3.0)]);
    // gaps == step, not > step, so nothing inserted
    assert_eq!(
        pairs(&fill_linear(&s, 10)),
        vec![(0, 1.0), (10, 2.0), (20, 3.0)]
    );
}

#[test]
fn empty_and_single() {
    let e = TsSeries::<f64>::new();
    assert!(fill_linear(&e, 10).is_empty());
    let one = series(&[(5, 5.0)]);
    assert_eq!(pairs(&fill_linear(&one, 10)), vec![(5, 5.0)]);
}

#[test]
fn step_zero_no_fill() {
    let s = series(&[(0, 0.0), (100, 1.0)]);
    assert_eq!(pairs(&fill_linear(&s, 0)), vec![(0, 0.0), (100, 1.0)]);
}

#[test]
fn huge_step_no_fill() {
    let s = series(&[(0, 0.0), (100, 1.0)]);
    assert_eq!(pairs(&fill_linear(&s, 1_000)), vec![(0, 0.0), (100, 1.0)]);
}

#[test]
fn partial_step_remainder() {
    // gap of 25 with step 10: insert 10, 20 (not 30, which is past 25)
    let s = series(&[(0, 0.0), (25, 25.0)]);
    let p = pairs(&fill_linear(&s, 10));
    assert_eq!(p.len(), 4); // 0, 10, 20, 25
    assert_eq!(p[1].0, 10);
    assert_eq!(p[2].0, 20);
    assert_eq!(p[3], (25, 25.0));
}

#[test]
fn multiple_gaps() {
    let s = series(&[(0, 0.0), (20, 2.0), (21, 9.0), (60, 6.0)]);
    let p = pairs(&fill_linear(&s, 10));
    // gap 0-20 -> insert 10; gap 20-21 none; gap 21-60 -> insert 31,41,51
    let ts: Vec<i64> = p.iter().map(|x| x.0).collect();
    assert_eq!(ts, vec![0, 10, 20, 21, 31, 41, 51, 60]);
}

#[test]
fn output_strictly_increasing() {
    let s = series(&[(0, 0.0), (37, 3.7), (90, 9.0)]);
    let p = pairs(&fill_linear(&s, 10));
    for w in p.windows(2) {
        assert!(w[1].0 > w[0].0, "ts must strictly increase: {:?}", w);
    }
}
