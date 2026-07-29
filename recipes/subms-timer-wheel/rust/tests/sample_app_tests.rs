//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! TIF expiry fires only uncancelled orders, and each optional feature holds
//! its own contract.

use subms_timer_wheel::TimerWheel;

#[test]
fn tif_expiry_scenario() {
    let mut expiries: TimerWheel<&'static str> = TimerWheel::new(256);
    let _a = expiries.schedule(3, "ORD-A");
    let b = expiries.schedule(5, "ORD-B");
    let _c = expiries.schedule(10, "ORD-C");
    assert_eq!(expiries.pending(), 3);

    expiries.cancel(b);

    let mut expired = Vec::new();
    for _ in 0..10 {
        expired.extend(expiries.tick());
    }
    assert_eq!(expired, vec!["ORD-A", "ORD-C"], "cancelled TIF never fires");
    assert_eq!(expiries.pending(), 0, "every timer retired");
}

#[cfg(feature = "hierarchical")]
#[test]
fn hierarchical_gtd_fires_and_cascades() {
    use subms_timer_wheel::HierarchicalTimerWheel;
    let mut gtd: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();
    gtd.schedule(30, "GTD-near");
    gtd.schedule(300, "GTD-far");

    let mut near_at = None;
    let mut far_at = None;
    for t in 1..=300 {
        for id in gtd.tick() {
            match id {
                "GTD-near" => near_at = Some(t),
                "GTD-far" => far_at = Some(t),
                _ => {}
            }
        }
    }
    assert_eq!(near_at, Some(30));
    assert_eq!(far_at, Some(300));
    assert!(gtd.cascades() >= 1, "the far order cascaded down a level");
}

#[cfg(feature = "concurrent")]
#[test]
fn concurrent_quote_timeouts_all_fire() {
    use std::thread;
    use subms_timer_wheel::ConcurrentTimerWheel;
    let wheel: ConcurrentTimerWheel<usize> = ConcurrentTimerWheel::new(256);
    let feeds = 4;
    let per_feed = 50;
    let mut handles = Vec::new();
    for feed in 0..feeds {
        let wheel = wheel.clone();
        handles.push(thread::spawn(move || {
            for i in 0..per_feed {
                wheel.schedule(1 + (i % 8), feed * 1000 + i);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let mut fired = 0usize;
    for _ in 0..16 {
        fired += wheel.tick().len();
    }
    assert_eq!(fired, feeds * per_feed);
}

#[cfg(feature = "deadline-scheduler")]
#[test]
fn deadline_heartbeat_fires_at_instant() {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;
    use subms_timer_wheel::{Clock, DeadlineScheduler};

    struct SharedClock(Rc<Cell<u64>>);
    impl Clock for SharedClock {
        fn now_nanos(&self) -> u64 {
            self.0.get()
        }
    }

    let now = Rc::new(Cell::new(0u64));
    let mut sched =
        DeadlineScheduler::new(64, SharedClock(Rc::clone(&now)), Duration::from_millis(1));
    sched.schedule_at(Duration::from_millis(5).as_nanos() as u64, "HEARTBEAT");

    now.set(Duration::from_millis(4).as_nanos() as u64);
    assert!(sched.poll().is_empty(), "nothing before the deadline");

    now.set(Duration::from_millis(5).as_nanos() as u64);
    assert_eq!(sched.poll(), vec!["HEARTBEAT"]);
}

#[cfg(feature = "cron")]
#[test]
fn cron_risk_snapshot_re_arms() {
    use subms_timer_wheel::{CronSchedule, CronScheduler};
    let schedule = CronSchedule::parse("*/5 * * * *").unwrap();
    let start = 1_704_067_201;
    let mut scheduler = CronScheduler::new(schedule, start);

    let first = scheduler.next_fire(start).unwrap();
    assert_eq!(first, 1_704_067_500);
    scheduler.record_fire(first);
    let second = scheduler.next_fire(first).unwrap();
    assert_eq!(second, first + 300);
}

#[cfg(feature = "metrics")]
#[test]
fn metered_expiry_counters() {
    use subms_timer_wheel::MeteredTimerWheel;
    let mut wheel: MeteredTimerWheel<&'static str> = MeteredTimerWheel::new(64);
    let _a = wheel.schedule(2, "ORD-A");
    let b = wheel.schedule(2, "ORD-B");
    wheel.cancel(b);
    let mut fired = 0usize;
    for _ in 0..3 {
        fired += wheel.tick().len();
    }
    let m = wheel.metrics();
    assert_eq!(m.scheduled, 2);
    assert_eq!(m.cancelled, 1);
    assert_eq!(fired, 1);
    assert_eq!(m.fired, 1);
    assert_eq!(m.ticks, 3);
}
