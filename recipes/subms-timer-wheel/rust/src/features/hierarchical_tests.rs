use super::*;

#[test]
fn short_delay_fires_on_correct_tick() {
    let mut w: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();
    w.schedule(5, "a");
    for _ in 0..4 {
        assert!(w.tick().is_empty());
    }
    assert_eq!(w.tick(), vec!["a"]);
}

#[test]
fn now_advances_one_unit_per_tick() {
    let mut w: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();
    assert_eq!(w.now(), 0);
    w.tick();
    w.tick();
    assert_eq!(w.now(), 2);
}

#[test]
fn cascade_boundary_64_ticks_fires_correctly() {
    // 64 ticks lands on level 1 (range >=64). After 64 ticks of
    // ticking, exactly one cascade event must have moved it to
    // level 0 and it must fire on the 64th tick.
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    w.schedule(64, 7);
    let mut fired_at: Option<u64> = None;
    for i in 1..=70 {
        let fired = w.tick();
        if !fired.is_empty() {
            assert_eq!(fired, vec![7]);
            fired_at = Some(i);
            break;
        }
    }
    assert_eq!(fired_at, Some(64));
    assert!(w.cascades() >= 1, "expected at least one cascade event");
}

#[test]
fn cascade_boundary_4096_ticks_fires_correctly() {
    // 4096 = SLOTS*SLOTS - on the upper edge of level 1 / lower of level 2.
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    w.schedule(4096, 42);
    let mut fired_at: Option<u64> = None;
    for i in 1..=4100 {
        if !w.tick().is_empty() {
            fired_at = Some(i);
            break;
        }
    }
    assert_eq!(fired_at, Some(4096));
    // The entry started on level 2 and cascaded toward level 0
    // before firing - at least one cascade event recorded.
    assert!(w.cascades() >= 1);
}

#[test]
fn cancel_before_fire_drops_value() {
    let mut w: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();
    let id = w.schedule(10, "doomed");
    assert!(w.cancel(id));
    for _ in 0..20 {
        assert!(w.tick().is_empty());
    }
}

#[test]
fn cancel_after_fire_returns_false() {
    let mut w: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();
    let id = w.schedule(2, "x");
    w.tick();
    let fired = w.tick();
    assert_eq!(fired, vec!["x"]);
    assert!(!w.cancel(id), "cancel after fire must return false");
}

#[test]
fn cancel_unknown_id_returns_false() {
    let mut w: HierarchicalTimerWheel<()> = HierarchicalTimerWheel::new();
    assert!(!w.cancel(99_999));
}

#[test]
fn long_delay_uses_coarse_wheel_then_cascades() {
    // Half-day-ish delay - lands deep on level 2, must cascade
    // down through level 1 and level 0 to fire.
    let delay: u64 = 5000;
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    w.schedule(delay, 1);
    let mut found = None;
    for i in 1..=(delay + 5) {
        if !w.tick().is_empty() {
            found = Some(i);
            break;
        }
    }
    assert_eq!(found, Some(delay));
}

#[test]
fn overflow_delay_rejected_by_try_schedule() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    let too_big = HierarchicalTimerWheel::<u32>::max_delay() as u64;
    match w.try_schedule(too_big, 1) {
        Err(TimerError::DelayTooLong { delay, max }) => {
            assert_eq!(delay, too_big);
            assert_eq!(max, too_big);
        }
        other => panic!("expected DelayTooLong, got {other:?}"),
    }
    assert!(w.try_schedule(too_big - 1, 1).is_ok());
}

#[test]
fn pending_tracks_live_timers_across_cascade() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    assert!(w.is_empty());
    w.schedule(5, 1);
    let far = w.schedule(300, 2);
    assert_eq!(w.pending(), 2);
    for _ in 0..5 {
        w.tick();
    }
    assert_eq!(w.pending(), 1, "the near timer fired");
    assert!(w.cancel(far));
    assert_eq!(w.pending(), 0);
}

#[test]
fn reschedule_moves_a_timer_across_levels() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    let id = w.schedule(5000, 9);
    assert!(w.reschedule(id, 3), "pull a far timer in to level 0");
    assert_eq!(w.pending(), 1);
    w.tick();
    w.tick();
    assert_eq!(w.tick(), vec![9]);
    assert!(!w.reschedule(id, 3), "a fired timer cannot be rescheduled");
}

#[test]
fn reschedule_unknown_id_returns_false() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    w.schedule(4, 1);
    assert!(!w.reschedule(4242, 9));
}

#[test]
fn drain_hands_back_every_pending_timer() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    for d in [2u64, 70, 5000] {
        w.schedule(d, d as u32);
    }
    let cancelled = w.schedule(9, 999);
    assert!(w.cancel(cancelled));

    let mut drained = w.drain();
    drained.sort();
    assert_eq!(drained, vec![2, 70, 5000]);
    assert_eq!(w.pending(), 0);
    for _ in 0..6000 {
        assert!(w.tick().is_empty());
    }
}

#[test]
fn clear_resets_the_tick_counter_and_drops_timers() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    w.schedule(100, 1);
    w.tick();
    w.tick();
    w.clear();
    assert_eq!(w.now(), 0);
    assert_eq!(w.pending(), 0);
    w.schedule(3, 2);
    w.tick();
    w.tick();
    assert_eq!(w.tick(), vec![2]);
}

#[test]
fn many_timers_fire_at_correct_distinct_ticks() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    for d in 1u32..=200 {
        w.schedule(d as u64, d);
    }
    let mut seen_total = 0;
    for i in 1..=200 {
        let fired = w.tick();
        for v in &fired {
            assert_eq!(*v, i as u32, "expected delay {i} to fire on tick {i}");
        }
        seen_total += fired.len();
    }
    assert_eq!(seen_total, 200);
}

#[test]
fn cascades_counter_zero_for_short_delays() {
    let mut w: HierarchicalTimerWheel<u32> = HierarchicalTimerWheel::new();
    w.schedule(3, 1);
    w.tick();
    w.tick();
    w.tick();
    assert_eq!(w.cascades(), 0);
}
