use subms_gorilla_block::TsGorillaBlock;
use subms_zone_map::{TsValueOp, TsValuePredicate, TsZone, TsZoneMap};

fn block(base: i64, n: i64, f: impl Fn(i64) -> f64) -> TsGorillaBlock {
    let mut b = TsGorillaBlock::new();
    for i in 0..n {
        b.append(base + i, f(i));
    }
    b
}

#[test]
fn observe_records_stats() {
    let mut z = TsZoneMap::new();
    z.observe(1, &block(1_000, 100, |i| i as f64));
    assert_eq!(z.len(), 1);
    let zone = z.zones()[0];
    assert_eq!(zone.block_id, 1);
    assert_eq!(zone.ts_min, 1_000);
    assert_eq!(zone.ts_max, 1_099);
    assert_eq!(zone.value_min, 0.0);
    assert_eq!(zone.value_max, 99.0);
    assert_eq!(zone.count, 100);
}

#[test]
fn empty_block_skipped() {
    let mut z = TsZoneMap::new();
    z.observe(1, &TsGorillaBlock::new());
    assert!(z.is_empty());
}

#[test]
fn time_window_pruning() {
    let mut z = TsZoneMap::new();
    z.observe(1, &block(0, 100, |i| i as f64));
    z.observe(2, &block(1_000, 100, |i| i as f64));
    z.observe(3, &block(2_000, 100, |i| i as f64));
    assert_eq!(z.candidates(1_000, 1_050, None), vec![2]);
    assert_eq!(z.candidates(50, 2_050, None), vec![1, 2, 3]);
    assert!(z.candidates(5_000, 6_000, None).is_empty());
    // touching boundary counts (inclusive)
    assert_eq!(z.candidates(1_099, 2_000, None), vec![2, 3]);
}

#[test]
fn lo_gt_hi_returns_empty() {
    let mut z = TsZoneMap::new();
    z.observe(1, &block(0, 10, |i| i as f64));
    assert!(z.candidates(100, 0, None).is_empty());
}

#[test]
fn value_predicate_pruning() {
    let mut z = TsZoneMap::new();
    z.observe(1, &block(0, 100, |i| i as f64)); // values 0..99
    z.observe(2, &block(1_000, 100, |i| 100.0 + i as f64)); // values 100..199

    // value > 150: only block 2 can hold it
    let gt = TsValuePredicate::new(TsValueOp::Gt, 150.0);
    assert_eq!(z.candidates(0, 2_000, Some(gt)), vec![2]);

    // value < 50: only block 1
    let lt = TsValuePredicate::new(TsValueOp::Lt, 50.0);
    assert_eq!(z.candidates(0, 2_000, Some(lt)), vec![1]);

    // value == 99: in block 1's range, not block 2's
    let eq = TsValuePredicate::new(TsValueOp::Eq, 99.0);
    assert_eq!(z.candidates(0, 2_000, Some(eq)), vec![1]);

    // value >= 200: neither (max is 199)
    let ge = TsValuePredicate::new(TsValueOp::Ge, 200.0);
    assert!(z.candidates(0, 2_000, Some(ge)).is_empty());

    // value <= 199: both
    let le = TsValuePredicate::new(TsValueOp::Le, 199.0);
    assert_eq!(z.candidates(0, 2_000, Some(le)), vec![1, 2]);
}

#[test]
fn combined_time_and_value() {
    let mut z = TsZoneMap::new();
    z.observe(1, &block(0, 100, |i| i as f64));
    z.observe(2, &block(1_000, 100, |i| 100.0 + i as f64));
    // window excludes block 2 even though its values would match
    let gt = TsValuePredicate::new(TsValueOp::Gt, 150.0);
    assert!(z.candidates(0, 99, Some(gt)).is_empty());
}

#[test]
fn observe_zone_direct() {
    let mut z = TsZoneMap::new();
    z.observe_zone(TsZone {
        block_id: 42,
        ts_min: 10,
        ts_max: 20,
        value_min: 1.0,
        value_max: 5.0,
        count: 11,
    });
    assert_eq!(z.candidates(15, 18, None), vec![42]);
}

#[test]
fn prunes_large_index_fast() {
    // 100k blocks; only a handful overlap the query window.
    let mut z = TsZoneMap::with_capacity(100_000);
    for id in 0..100_000u64 {
        let base = id as i64 * 1_000;
        z.observe_zone(TsZone {
            block_id: id,
            ts_min: base,
            ts_max: base + 999,
            value_min: 0.0,
            value_max: id as f64,
            count: 1_000,
        });
    }
    let got = z.candidates(5_000, 7_500, None);
    assert_eq!(got, vec![5, 6, 7]);
}

#[test]
fn clear_resets() {
    let mut z = TsZoneMap::new();
    z.observe(1, &block(0, 10, |i| i as f64));
    z.clear();
    assert!(z.is_empty());
}
