use super::*;

#[test]
fn empty_decay_is_zero() {
    let clk = ManualClock::new();
    let h = DecayingHdrHistogram::new(3, 1_000_000_000, &clk);
    assert_eq!(h.count() as u64, 0);
    assert_eq!(h.max(), 0);
    assert_eq!(h.value_at_percentile(0.99), 0);
}

#[test]
fn halflife_accessor_reports_configured_value() {
    let clk = ManualClock::new();
    let h = DecayingHdrHistogram::new(3, 750_000_000, &clk);
    assert_eq!(h.halflife_ns(), 750_000_000);
    // A zero half-life is clamped up to 1 so the decay factor stays finite.
    let z = DecayingHdrHistogram::new(3, 0, &clk);
    assert_eq!(z.halflife_ns(), 1);
}

#[test]
fn no_time_passing_means_no_decay() {
    let clk = ManualClock::new();
    let mut h = DecayingHdrHistogram::new(3, 1_000_000_000, &clk);
    for v in 1u64..=100 {
        h.record(v);
    }
    let c = h.count();
    assert!((c - 100.0).abs() < 1e-6, "no time passed, count={c}");
}

#[test]
fn one_halflife_halves_count() {
    let clk = ManualClock::new();
    let halflife = 1_000_000_000u64;
    let mut h = DecayingHdrHistogram::new(3, halflife, &clk);
    for _ in 0..1000 {
        h.record(50);
    }
    // Pre-decay count: 1000.
    clk.advance_ns(halflife);
    let c = h.count();
    // After one half-life, count should be ~500.
    assert!(
        (c - 500.0).abs() < 1.0,
        "halflife should halve: count={c}, expected ~500"
    );
}

#[test]
fn two_halflives_quarter_count() {
    let clk = ManualClock::new();
    let halflife = 500_000_000u64;
    let mut h = DecayingHdrHistogram::new(3, halflife, &clk);
    for _ in 0..1000 {
        h.record(100);
    }
    clk.advance_ns(halflife * 2);
    let c = h.count();
    assert!(
        (c - 250.0).abs() < 1.0,
        "two halflives -> 1/4: count={c}, expected ~250"
    );
}

#[test]
fn recent_records_outweigh_old() {
    let clk = ManualClock::new();
    let halflife = 1_000_000_000u64;
    let mut h = DecayingHdrHistogram::new(3, halflife, &clk);
    // 100 records at value=10, then 4 half-lives pass.
    for _ in 0..100 {
        h.record(10);
    }
    clk.advance_ns(halflife * 4);
    // 100 records at value=1000.
    for _ in 0..100 {
        h.record(1000);
    }
    // Old records weigh ~100 * 0.0625 = 6.25; new weigh 100.
    // p50 should land in the recent (high) bucket.
    let p50 = h.value_at_percentile(0.5);
    assert!(p50 >= 500, "recent bucket dominates: p50={p50}");
}

#[test]
fn long_idle_collapses_to_zero() {
    let clk = ManualClock::new();
    let halflife = 1_000_000_000u64;
    let mut h = DecayingHdrHistogram::new(3, halflife, &clk);
    for _ in 0..1000 {
        h.record(50);
    }
    // 30 half-lives - the count should be vanishing.
    clk.advance_ns(halflife * 30);
    let c = h.count();
    assert!(c < 1e-6, "30 half-lives -> ~0: count={c}");
}

#[test]
fn write_during_decay_competes_fairly() {
    let clk = ManualClock::new();
    let halflife = 1_000_000_000u64;
    let mut h = DecayingHdrHistogram::new(3, halflife, &clk);
    h.record(100);
    clk.advance_ns(halflife);
    h.record(200);
    // After one half-life, the original write counts ~0.5; the
    // new write counts 1.0. p50 should reflect the newer value.
    let total = h.count();
    assert!(
        (total - 1.5).abs() < 0.05,
        "weighted total ~ 1.5: got {total}"
    );
}
