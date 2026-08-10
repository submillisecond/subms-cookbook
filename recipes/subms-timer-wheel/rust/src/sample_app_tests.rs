//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! TIF expiry fires only uncancelled orders, and each optional feature holds
//! its own contract.

use super::*;

#[test]
fn tif_supervisor_scenario() {
    let mut expiries: TimerWheel<&'static str> = TimerWheel::new(256);
    let _a = expiries.schedule(3, "ORD-A");
    let b = expiries.schedule(5, "ORD-B");
    let c = expiries.schedule(9, "ORD-C");
    let _d = expiries.schedule(12, "ORD-D");
    assert_eq!(expiries.pending(), 4);

    let mut expired = Vec::new();
    expired.extend(expiries.advance(2));
    expiries.cancel(b); // filled at t=2

    expired.extend(expiries.advance(2));
    expiries.reschedule(c, 6); // amended at t=4, now due t=10

    expired.extend(expiries.advance(7));
    assert_eq!(
        expired,
        vec!["ORD-A", "ORD-C"],
        "a cancelled TIF never fires"
    );
    assert_eq!(
        expiries.drain(),
        vec!["ORD-D"],
        "the session closes on the long TIF"
    );
    assert_eq!(expiries.pending(), 0, "every timer retired");
}

#[cfg(feature = "hierarchical")]
#[test]
fn hierarchical_gtd_fires_and_cascades() {
    use crate::HierarchicalTimerWheel;
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
    use crate::ConcurrentTimerWheel;
    use std::thread;
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
fn deadline_session_idle_timeout_is_bumped_by_traffic() {
    use crate::{DeadlineScheduler, TestClock};
    use std::time::Duration;

    let idle = Duration::from_millis(30);
    let mut sched: DeadlineScheduler<&'static str, TestClock> =
        DeadlineScheduler::new(256, TestClock::new(), Duration::from_millis(1));
    let session = sched.schedule_after(idle, "SESSION-1");

    for gap in [10u64, 15] {
        sched.clock().advance(Duration::from_millis(gap));
        assert!(sched.poll().is_empty(), "traffic keeps the session alive");
        assert!(sched.reschedule_after(session, idle));
    }

    sched.clock().advance(Duration::from_millis(30));
    assert_eq!(sched.poll(), vec!["SESSION-1"]);
}

#[cfg(feature = "cron")]
#[test]
fn cron_risk_snapshot_re_arms() {
    use crate::{CronSchedule, CronScheduler};
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
    use crate::MeteredTimerWheel;
    let mut wheel: MeteredTimerWheel<&'static str> = MeteredTimerWheel::new(64);
    let _a = wheel.schedule(2, "ORD-A");
    let b = wheel.schedule(2, "ORD-B");
    let c = wheel.schedule(2, "ORD-C");
    wheel.cancel(b);
    wheel.reschedule(c, 20);

    let fired = wheel.advance(3).len();
    let left = wheel.drain();
    let m = wheel.metrics();
    assert_eq!(m.scheduled, 3);
    assert_eq!(m.cancelled, 1);
    assert_eq!(m.rescheduled, 1);
    assert_eq!(fired, 1);
    assert_eq!(m.fired, 1);
    assert_eq!(left, vec!["ORD-C"]);
    assert_eq!(m.drained, 1);
    assert_eq!(m.ticks, 3);
}
