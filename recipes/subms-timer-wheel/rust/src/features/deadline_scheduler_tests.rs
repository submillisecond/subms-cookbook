use super::*;

fn sched_with_clock() -> (DeadlineScheduler<&'static str, TestClock>, ()) {
    (
        DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1)),
        (),
    )
}

#[test]
fn tick_nanos_reflects_the_configured_resolution() {
    let s: DeadlineScheduler<&'static str, TestClock> =
        DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
    assert_eq!(s.tick_nanos(), 1_000_000);
}

#[test]
fn schedule_after_fires_after_elapsed_time() {
    let (mut s, _) = sched_with_clock();
    s.schedule_after(Duration::from_millis(3), "a");
    // 0 ms elapsed - nothing fires.
    assert!(s.poll().is_empty());
    // 2 ms elapsed - still nothing.
    s.clock.advance(Duration::from_millis(2));
    assert!(s.poll().is_empty());
    // 3 ms elapsed total - fires.
    s.clock.advance(Duration::from_millis(1));
    assert_eq!(s.poll(), vec!["a"]);
}

#[test]
fn schedule_at_with_absolute_deadline_fires_when_clock_passes_it() {
    let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
    let when = s.clock.now_nanos() + Duration::from_millis(5).as_nanos() as u64;
    s.schedule_at(when, "five");
    s.clock.advance(Duration::from_millis(4));
    assert!(s.poll().is_empty());
    s.clock.advance(Duration::from_millis(1));
    assert_eq!(s.poll(), vec!["five"]);
}

#[test]
fn schedule_at_in_the_past_fires_on_next_tick() {
    let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
    s.clock.advance(Duration::from_secs(10));
    let id = s.schedule_at(0, "stale");
    s.clock.advance(Duration::from_millis(1));
    assert_eq!(s.poll(), vec!["stale"]);
    // The id should be gone from cancellation tracking.
    assert!(!s.cancel(id));
}

#[test]
fn cancel_removes_before_fire() {
    let (mut s, _) = sched_with_clock();
    let id = s.schedule_after(Duration::from_millis(3), "doomed");
    assert!(s.cancel(id));
    s.clock.advance(Duration::from_millis(10));
    assert!(s.poll().is_empty());
}

#[test]
fn poll_with_no_clock_movement_is_idempotent() {
    let (mut s, _) = sched_with_clock();
    s.schedule_after(Duration::from_millis(2), "a");
    assert!(s.poll().is_empty());
    assert!(s.poll().is_empty());
    s.clock.advance(Duration::from_millis(2));
    let first = s.poll();
    let second = s.poll();
    assert_eq!(first, vec!["a"]);
    assert!(second.is_empty(), "second poll must not refire");
}

#[test]
fn sub_tick_delay_rounds_up_to_one_tick() {
    let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
    // 500 us < 1 ms tick - must still fire on the next tick.
    s.schedule_after(Duration::from_micros(500), "a");
    s.clock.advance(Duration::from_millis(1));
    assert_eq!(s.poll(), vec!["a"]);
}

#[test]
fn many_deadlines_fire_in_order() {
    let mut s = DeadlineScheduler::new(64, TestClock::new(), Duration::from_millis(1));
    for i in 1u32..=10 {
        s.schedule_after(Duration::from_millis(i as u64), i);
    }
    for i in 1u32..=10 {
        s.clock.advance(Duration::from_millis(1));
        assert_eq!(s.poll(), vec![i]);
    }
}

#[test]
fn monotonic_clock_default_does_not_panic() {
    // Smoke test - production clock is hard to assert against; just
    // exercise the `now_nanos` path and confirm it monotonically
    // advances (or stays equal) across two calls.
    let c = MonotonicClock::new();
    let a = c.now_nanos();
    let b = c.now_nanos();
    assert!(b >= a);
}

#[test]
fn monotonic_clock_default_origin_lazily_initialises() {
    // `Default` leaves `origin: None`; `now_nanos` then falls back to
    // `Instant::now` for the origin. Exercise that branch + the Default
    // and TestClock::default constructors.
    let c = MonotonicClock::default();
    let a = c.now_nanos();
    let b = c.now_nanos();
    assert!(b >= a);

    let t = TestClock::default();
    assert_eq!(t.now_nanos(), 0);
    t.advance(Duration::from_nanos(7));
    assert_eq!(t.now_nanos(), 7);
}
