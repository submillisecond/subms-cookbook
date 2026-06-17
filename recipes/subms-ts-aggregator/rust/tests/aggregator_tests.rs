use subms_ts_aggregator::TsWindowedAggregator;

#[test]
fn empty_aggregator() {
    let a = TsWindowedAggregator::new(1_000);
    assert!(a.is_empty());
    assert_eq!(a.count(), 0);
    assert_eq!(a.min(), None);
    assert_eq!(a.max(), None);
    assert_eq!(a.mean(), None);
    assert_eq!(a.sum(), 0.0);
}

#[test]
fn basic_aggregates() {
    let mut a = TsWindowedAggregator::new(10_000);
    a.push(0, 5.0);
    a.push(1, 1.0);
    a.push(2, 9.0);
    a.push(3, 3.0);
    assert_eq!(a.count(), 4);
    assert_eq!(a.min(), Some(1.0));
    assert_eq!(a.max(), Some(9.0));
    assert_eq!(a.sum(), 18.0);
    assert_eq!(a.mean(), Some(4.5));
}

#[test]
fn window_expiry() {
    let mut a = TsWindowedAggregator::new(1_000);
    a.push(0, 5.0);
    a.push(500, 1.0);
    a.push(900, 9.0);
    assert_eq!(a.count(), 3);
    // push at 1500: cutoff = 500, so ts 0 and ts 500 expire (t <= cutoff)
    a.push(1_500, 2.0);
    assert_eq!(a.count(), 2); // 900, 1500
    assert_eq!(a.min(), Some(2.0));
    assert_eq!(a.max(), Some(9.0));
    assert_eq!(a.sum(), 11.0);
}

#[test]
fn min_max_recover_after_expiry() {
    // The min leaves the window; min must recompute from survivors.
    let mut a = TsWindowedAggregator::new(100);
    a.push(0, 1.0); // the min
    a.push(50, 5.0);
    a.push(90, 3.0);
    assert_eq!(a.min(), Some(1.0));
    a.push(101, 4.0); // cutoff = 1 -> ts 0 expires
    assert_eq!(a.min(), Some(3.0));
    assert_eq!(a.max(), Some(5.0));
}

#[test]
fn max_recovers_after_expiry() {
    let mut a = TsWindowedAggregator::new(100);
    a.push(0, 9.0); // the max
    a.push(50, 2.0);
    a.push(90, 4.0);
    assert_eq!(a.max(), Some(9.0));
    a.push(101, 1.0); // ts 0 expires
    assert_eq!(a.max(), Some(4.0));
    assert_eq!(a.min(), Some(1.0));
}

#[test]
fn duplicate_values_handled() {
    let mut a = TsWindowedAggregator::new(10_000);
    for t in 0..10 {
        a.push(t, 5.0);
    }
    assert_eq!(a.min(), Some(5.0));
    assert_eq!(a.max(), Some(5.0));
    assert_eq!(a.count(), 10);
    assert_eq!(a.sum(), 50.0);
}

#[test]
fn window_iter_in_order() {
    let mut a = TsWindowedAggregator::new(1_000);
    a.push(10, 1.0);
    a.push(20, 2.0);
    a.push(30, 3.0);
    let pts: Vec<(i64, f64)> = a.window().map(|p| (p.ts, p.value)).collect();
    assert_eq!(pts, vec![(10, 1.0), (20, 2.0), (30, 3.0)]);
}

#[test]
fn merge_partitions() {
    let mut a = TsWindowedAggregator::new(10_000);
    a.push(0, 1.0);
    a.push(100, 3.0);
    let mut b = TsWindowedAggregator::new(10_000);
    b.push(50, 9.0);
    b.push(150, 2.0);
    let m = a.merge(&b);
    assert_eq!(m.count(), 4);
    assert_eq!(m.min(), Some(1.0));
    assert_eq!(m.max(), Some(9.0));
    assert_eq!(m.sum(), 15.0);
    // ordered union
    let ts: Vec<i64> = m.window().map(|p| p.ts).collect();
    assert_eq!(ts, vec![0, 50, 100, 150]);
}

#[test]
fn merge_applies_window_expiry() {
    // Combined window expires old points relative to the latest across both.
    let mut a = TsWindowedAggregator::new(100);
    a.push(0, 1.0);
    let mut b = TsWindowedAggregator::new(100);
    b.push(200, 2.0);
    let m = a.merge(&b);
    // latest is 200, cutoff 100 -> ts 0 expired
    assert_eq!(m.count(), 1);
    assert_eq!(m.min(), Some(2.0));
}

#[test]
fn sliding_correctness_vs_naive() {
    // Cross-check the streaming min/max/sum against a brute-force window.
    let window = 1_000i64;
    let mut a = TsWindowedAggregator::new(window);
    let mut points: Vec<(i64, f64)> = Vec::new();
    let mut state = 12345u64;
    for i in 0..5_000i64 {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let v = (state >> 40) as f64 / 1_000.0;
        let ts = i * 10;
        a.push(ts, v);
        points.push((ts, v));
        // brute force over the same window definition (ts > now - window)
        let cutoff = ts - window;
        let live: Vec<f64> = points
            .iter()
            .filter(|&&(t, _)| t > cutoff)
            .map(|&(_, v)| v)
            .collect();
        let bf_min = live.iter().cloned().fold(f64::INFINITY, f64::min);
        let bf_max = live.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        assert_eq!(a.min().unwrap(), bf_min, "min mismatch at i={i}");
        assert_eq!(a.max().unwrap(), bf_max, "max mismatch at i={i}");
        assert_eq!(a.count(), live.len());
    }
}

// ---------- distributed-merge wire format ----------

use subms_ts_aggregator::TsAggWireError;

// Pins the cross-language wire layout: window 1000, points (100,1) (200,2)
// (300,3). The Java port asserts the identical hex.
const WIRE_FIXTURE: &str = "01e803000000000000030000006400000000000000000000000000f03fc80000000000000000000000000000402c010000000000000000000000000840";

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn wire_matches_fixture() {
    let mut a = TsWindowedAggregator::new(1_000);
    a.push(100, 1.0);
    a.push(200, 2.0);
    a.push(300, 3.0);
    assert_eq!(to_hex(&a.to_wire()), WIRE_FIXTURE);
}

#[test]
fn wire_round_trips() {
    let mut a = TsWindowedAggregator::new(1_000);
    for i in 0..500 {
        a.push(i * 3, (i as f64 * 0.5).sin());
    }
    let back = TsWindowedAggregator::from_wire(&a.to_wire()).unwrap();
    assert_eq!(back.window_ns(), a.window_ns());
    assert_eq!(back.count(), a.count());
    // min/max are stored values, exact across the round-trip; sum is a running
    // total whose add/subtract-on-expiry history differs from the fresh replay,
    // so it agrees only to FP tolerance.
    assert_eq!(back.min(), a.min());
    assert_eq!(back.max(), a.max());
    assert!((back.sum() - a.sum()).abs() <= a.sum().abs() * 1e-12 + 1e-9);
}

#[test]
fn merge_across_the_wire() {
    // Two shards, same logical window; coordinator decodes both + merges.
    let mut s1 = TsWindowedAggregator::new(1_000);
    let mut s2 = TsWindowedAggregator::new(1_000);
    for i in 0..200 {
        s1.push(i * 2, i as f64);
        s2.push(i * 2 + 1, i as f64 * 2.0);
    }
    let d1 = TsWindowedAggregator::from_wire(&s1.to_wire()).unwrap();
    let d2 = TsWindowedAggregator::from_wire(&s2.to_wire()).unwrap();
    let coordinator = d1.merge(&d2);
    let direct = s1.merge(&s2);
    assert_eq!(coordinator.count(), direct.count());
    assert_eq!(coordinator.sum(), direct.sum());
    assert_eq!(coordinator.min(), direct.min());
    assert_eq!(coordinator.max(), direct.max());
}

#[test]
fn wire_empty_window() {
    let a = TsWindowedAggregator::new(500);
    let back = TsWindowedAggregator::from_wire(&a.to_wire()).unwrap();
    assert_eq!(back.count(), 0);
    assert_eq!(back.window_ns(), 500);
}

#[test]
fn wire_rejects_bad_version() {
    let mut bytes = {
        let mut a = TsWindowedAggregator::new(1_000);
        a.push(1, 1.0);
        a.to_wire()
    };
    bytes[0] = 99;
    assert_eq!(
        TsWindowedAggregator::from_wire(&bytes).unwrap_err(),
        TsAggWireError::BadVersion(99)
    );
}

#[test]
fn wire_rejects_truncated() {
    assert_eq!(
        TsWindowedAggregator::from_wire(&[]).unwrap_err(),
        TsAggWireError::Truncated
    );
    let mut a = TsWindowedAggregator::new(1_000);
    a.push(1, 1.0);
    a.push(2, 2.0);
    let full = a.to_wire();
    // chop a point off the tail
    assert_eq!(
        TsWindowedAggregator::from_wire(&full[..full.len() - 4]).unwrap_err(),
        TsAggWireError::Truncated
    );
}
