use super::*;

#[test]
fn scheduled_counter_increments_on_schedule() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    w.schedule(1, 1);
    w.schedule(2, 2);
    assert_eq!(w.metrics().scheduled, 2);
}

#[test]
fn num_slots_reflects_the_configured_wheel_size() {
    let w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(256);
    assert_eq!(w.num_slots(), 256);
}

#[test]
fn fired_counter_matches_tick_results() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    w.schedule(1, 1);
    w.schedule(2, 2);
    w.schedule(2, 3);
    let fired_count: usize = (0..4).map(|_| w.tick().len()).sum();
    assert_eq!(fired_count as u64, w.metrics().fired);
    assert_eq!(w.metrics().fired, 3);
}

#[test]
fn cancelled_counter_only_increments_on_real_cancel() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    let id = w.schedule(5, 1);
    assert!(w.cancel(id));
    assert!(!w.cancel(9999));
    assert_eq!(w.metrics().cancelled, 1);
}

#[test]
fn ticks_counter_increments_per_tick() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    for _ in 0..7 {
        w.tick();
    }
    assert_eq!(w.metrics().ticks, 7);
}

#[test]
fn cascade_events_stays_zero_on_single_level_wheel() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    for d in 1..=20 {
        w.schedule(d, d as u32);
    }
    for _ in 0..30 {
        w.tick();
    }
    assert_eq!(w.metrics().cascade_events, 0);
}

#[test]
fn metrics_snapshot_independent_of_wheel_state() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    w.schedule(1, 1);
    let snap_a = w.metrics();
    w.schedule(2, 2);
    let snap_b = w.metrics();
    // The earlier snapshot is unchanged after a later schedule.
    assert_eq!(snap_a.scheduled, 1);
    assert_eq!(snap_b.scheduled, 2);
}

#[test]
fn schedule_fire_cancel_full_lifecycle_counters() {
    let mut w: MeteredTimerWheel<u32> = MeteredTimerWheel::new(64);
    let _a = w.schedule(2, 1);
    let b = w.schedule(2, 2);
    w.cancel(b);
    // tick #1: empty. tick #2: fires `1` (not `2` since cancelled).
    let fired_1 = w.tick();
    let fired_2 = w.tick();
    let total: Vec<u32> = fired_1.into_iter().chain(fired_2).collect();
    assert_eq!(total, vec![1]);
    let m = w.metrics();
    assert_eq!(m.scheduled, 2);
    assert_eq!(m.cancelled, 1);
    assert_eq!(m.fired, 1);
    assert_eq!(m.ticks, 2);
}
