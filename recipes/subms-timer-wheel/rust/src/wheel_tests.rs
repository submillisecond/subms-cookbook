use super::*;

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
    assert_eq!(w.pending(), 0, "cancel retires the id immediately");
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
fn cancel_twice_returns_false_the_second_time() {
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    let id = w.schedule(3, "a");
    assert!(w.cancel(id));
    assert!(!w.cancel(id));
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
fn delay_that_is_an_exact_multiple_of_the_slot_count_fires_on_time() {
    // The slot for a multiple-of-N delay is the bucket the hand just left,
    // so it is not revisited for a full revolution. Charging a rounds
    // counter for that revolution too would fire a lap late.
    for slots in [16usize, 64] {
        for laps in 1..=3usize {
            let mut w: TimerWheel<usize> = TimerWheel::new(slots);
            let delay = slots * laps;
            w.schedule(delay, delay);
            for t in 1..delay {
                assert!(w.tick().is_empty(), "slots={slots} delay={delay} tick={t}");
            }
            assert_eq!(w.tick(), vec![delay], "slots={slots} delay={delay}");
        }
    }
}

#[test]
fn every_delay_up_to_three_revolutions_fires_on_its_tick() {
    let slots = 8usize;
    for delay in 1..=(3 * slots) {
        let mut w: TimerWheel<usize> = TimerWheel::new(slots);
        w.schedule(delay, delay);
        let mut fired_at = None;
        for t in 1..=(4 * slots) {
            if !w.tick().is_empty() {
                fired_at = Some(t);
                break;
            }
        }
        assert_eq!(fired_at, Some(delay), "delay {delay}");
    }
}

#[test]
fn slots_rounded_up_to_power_of_two() {
    assert_eq!(TimerWheel::<u32>::new(1000).num_slots(), 1024);
    assert_eq!(TimerWheel::<u32>::new(0).num_slots(), 2);
}

#[test]
fn delay_zero_fires_on_the_next_tick() {
    // A deadline already in the past is due immediately; Netty treats it the
    // same way. The wheel's finest resolution is one tick, so "immediately"
    // means the next tick.
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    w.schedule(0, "now");
    assert_eq!(w.tick(), vec!["now"]);
}

#[test]
fn cancel_returns_false_on_already_fired() {
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    let id = w.schedule(1, "x");
    assert_eq!(w.tick(), vec!["x"]);
    assert!(!w.cancel(id), "can't cancel an already-fired timer");
}

#[test]
fn many_pending_timers_fire_at_various_delays() {
    // Delays 1..=50 fire on tick(delay); 50 ticks catches all.
    let mut w: TimerWheel<u32> = TimerWheel::new(128);
    for i in 1..=50u32 {
        w.schedule(i as usize, i);
    }
    assert_eq!(w.pending(), 50);
    let mut total_fired = 0usize;
    for _ in 0..50 {
        total_fired += w.tick().len();
    }
    assert_eq!(total_fired, 50);
    assert_eq!(w.pending(), 0);
}

#[test]
fn ticks_with_no_timers_return_empty() {
    let mut w: TimerWheel<u32> = TimerWheel::new(16);
    for _ in 0..100 {
        assert!(w.tick().is_empty());
    }
}

#[test]
fn advance_catches_up_and_returns_in_tick_order() {
    let mut w: TimerWheel<u32> = TimerWheel::new(64);
    w.schedule(1, 1);
    w.schedule(2, 2);
    w.schedule(3, 3);
    assert_eq!(w.advance(3), vec![1, 2, 3]);
    assert_eq!(w.advance(0), Vec::<u32>::new());
    assert!(w.is_empty());
}

#[test]
fn reschedule_moves_a_pending_timer_and_keeps_its_id() {
    let mut w: TimerWheel<&'static str> = TimerWheel::new(64);
    let id = w.schedule(2, "a");
    assert!(w.reschedule(id, 5));
    assert_eq!(w.pending(), 1);
    for _ in 0..4 {
        assert!(w.tick().is_empty());
    }
    assert_eq!(w.tick(), vec!["a"]);
    assert!(!w.cancel(id), "the id retired when the timer fired");
}

#[test]
fn reschedule_can_pull_a_timer_earlier() {
    let mut w: TimerWheel<u32> = TimerWheel::new(64);
    let id = w.schedule(40, 7);
    assert!(w.reschedule(id, 1));
    assert_eq!(w.tick(), vec![7]);
}

#[test]
fn reschedule_rejects_unknown_cancelled_and_fired_ids() {
    let mut w: TimerWheel<u32> = TimerWheel::new(64);
    assert!(!w.reschedule(42, 3), "unknown id");

    let cancelled = w.schedule(3, 1);
    w.cancel(cancelled);
    assert!(!w.reschedule(cancelled, 3), "cancelled id");

    let fired = w.schedule(1, 2);
    w.tick();
    assert!(!w.reschedule(fired, 3), "fired id");
}

#[test]
fn reschedule_survives_a_bucket_shared_with_other_timers() {
    // swap_remove reorders the bucket; the other timers must still fire.
    let mut w: TimerWheel<u32> = TimerWheel::new(8);
    let a = w.schedule(3, 1);
    w.schedule(3, 2);
    w.schedule(3, 3);
    assert!(w.reschedule(a, 6));
    let mut at_3 = w.advance(3);
    at_3.sort();
    assert_eq!(at_3, vec![2, 3]);
    assert_eq!(w.advance(3), vec![1]);
}

#[test]
fn drain_returns_every_pending_timer_and_empties_the_wheel() {
    let mut w: TimerWheel<u32> = TimerWheel::new(16);
    for i in 1..=20u32 {
        w.schedule(i as usize, i);
    }
    let cancelled = w.schedule(4, 999);
    w.cancel(cancelled);

    let mut drained = w.drain();
    drained.sort();
    assert_eq!(drained, (1..=20).collect::<Vec<u32>>());
    assert_eq!(w.pending(), 0);
    assert!(w.advance(64).is_empty(), "nothing left to fire");
}

#[test]
fn clear_drops_pending_timers_and_resets_the_hand() {
    let mut w: TimerWheel<u32> = TimerWheel::new(16);
    w.schedule(3, 1);
    w.advance(2);
    w.clear();
    assert_eq!(w.pending(), 0);
    // Hand back at 0, so a fresh delay-3 timer still fires on tick 3.
    w.schedule(3, 2);
    assert_eq!(w.advance(3), vec![2]);
}

#[test]
fn slot_len_shows_bucket_occupancy() {
    let mut w: TimerWheel<u32> = TimerWheel::new(8);
    w.schedule(3, 1);
    w.schedule(11, 2); // 11 & 7 == 3, same bucket, one revolution later
    assert_eq!(w.slot_len(3), 2);
    assert_eq!(w.slot_len(4), 0);
    assert_eq!(w.slot_len(9999), 0, "out of range reads as empty");
}

#[test]
fn try_schedule_refuses_a_delay_past_capacity() {
    let mut w: TimerWheel<u32> = TimerWheel::new(2);
    let max = w.max_delay();
    assert_eq!(max, 2 * i32::MAX as u64);
    match w.try_schedule(usize::MAX, 1) {
        Err(TimerError::DelayTooLong { delay, max: m }) => {
            assert_eq!(delay, usize::MAX as u64);
            assert_eq!(m, max);
        }
        other => panic!("expected DelayTooLong, got {other:?}"),
    }
    assert_eq!(w.pending(), 0, "a refused schedule arms nothing");
    assert!(w.try_schedule(4, 1).is_ok());
}

#[test]
fn schedule_clamps_a_delay_past_capacity_instead_of_refusing() {
    let mut w: TimerWheel<u32> = TimerWheel::new(4);
    let id = w.schedule(usize::MAX, 1);
    assert_eq!(w.pending(), 1);
    assert!(w.cancel(id));
}
