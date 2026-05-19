use subms_timer_wheel::TimerWheel;

#[test]
fn fires_at_correct_tick() {
    // delay=3 means the timer fires on the 3rd tick after scheduling.
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    w.schedule(3, "a");
    assert!(w.tick().is_empty());
    assert!(w.tick().is_empty());
    assert_eq!(w.tick(), vec!["a"]);
}

#[test]
fn cancels_pending_timer() {
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    let id = w.schedule(3, "a");
    assert!(w.cancel(id));
    for _ in 0..4 {
        assert!(w.tick().is_empty());
    }
}

#[test]
fn cancel_unknown_id_returns_false() {
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    assert!(!w.cancel(999));
}

#[test]
fn multiple_timers_same_tick_all_fire() {
    let mut w: TimerWheel<u32> = TimerWheel::new(64);
    w.schedule(2, 1);
    w.schedule(2, 2);
    w.schedule(2, 3);
    assert!(w.tick().is_empty());
    let mut fired = w.tick();
    fired.sort();
    assert_eq!(fired, vec![1, 2, 3]);
}

#[test]
fn long_delay_spans_revolutions() {
    let n = 16usize;
    let mut w: TimerWheel<&'static str> = TimerWheel::new(n);
    w.schedule(2 * n + 3, "later");
    for _ in 0..(2 * n + 2) {
        assert!(w.tick().is_empty());
    }
    assert_eq!(w.tick(), vec!["later"]);
}

#[test]
fn slots_rounded_up_to_power_of_two() {
    assert_eq!(TimerWheel::<u32>::new(1000).num_slots(), 1024);
    assert_eq!(TimerWheel::<u32>::new(0).num_slots(), 2);
}

#[test]
fn delay_zero_fires_on_next_tick_or_inline() {
    // delay=0 means slot=current. Depending on whether the hand has visited
    // it yet, it fires on the very next tick.
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    w.schedule(0, "now");
    // It will fire within at most num_slots ticks.
    let mut fired = false;
    for _ in 0..64 {
        if !w.tick().is_empty() {
            fired = true;
            break;
        }
    }
    assert!(fired);
}

#[test]
fn cancel_returns_false_on_already_fired() {
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    let id = w.schedule(0, "x");
    for _ in 0..64 {
        if !w.tick().is_empty() {
            break;
        }
    }
    assert!(!w.cancel(id), "can't cancel an already-fired timer");
}

#[test]
fn many_pending_timers_fire_at_various_delays() {
    // Delays 1..=50 fire on tick(delay); 50 ticks catches all.
    let mut w: TimerWheel<u32> = TimerWheel::new(128);
    for i in 1..=50u32 {
        w.schedule(i as usize, i);
    }
    let mut total_fired = 0usize;
    for _ in 0..50 {
        total_fired += w.tick().len();
    }
    assert_eq!(total_fired, 50);
}

#[test]
fn ticks_with_no_timers_return_empty() {
    let mut w: TimerWheel<u32> = TimerWheel::new(16);
    for _ in 0..100 {
        assert!(w.tick().is_empty());
    }
}
