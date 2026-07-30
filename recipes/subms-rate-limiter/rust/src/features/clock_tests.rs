use super::*;

#[test]
fn system_clock_is_monotonic_non_decreasing() {
    let c = SystemClock::new();
    let a = c.now_ns();
    let b = c.now_ns();
    assert!(b >= a, "SystemClock must not go backwards: {a} then {b}");
}

#[test]
fn system_clock_default_matches_new() {
    let c = SystemClock::default();
    // Just after construction the elapsed time is tiny; a second read is >=.
    let a = c.now_ns();
    let b = c.now_ns();
    assert!(b >= a);
}

#[test]
fn test_clock_starts_at_zero_and_advances() {
    let c = TestClock::new();
    assert_eq!(c.now_ns(), 0);
    c.advance(500);
    assert_eq!(c.now_ns(), 500);
    c.advance(500);
    assert_eq!(c.now_ns(), 1000);
}

#[test]
fn test_clock_default_starts_at_zero() {
    let c = TestClock::default();
    assert_eq!(c.now_ns(), 0);
}

#[test]
fn test_clock_with_start_seeds_origin() {
    let c = TestClock::with_start(1_234);
    assert_eq!(c.now_ns(), 1_234);
    c.advance(6);
    assert_eq!(c.now_ns(), 1_240);
}

#[test]
fn test_clock_advance_ms_scales_to_nanos() {
    let c = TestClock::new();
    c.advance_ms(3);
    assert_eq!(c.now_ns(), 3_000_000);
}

#[test]
fn test_clock_advance_saturates_at_u64_max() {
    let c = TestClock::with_start(u64::MAX - 1);
    c.advance(1_000);
    assert_eq!(c.now_ns(), u64::MAX);
}
