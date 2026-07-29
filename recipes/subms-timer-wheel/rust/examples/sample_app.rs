//! Sample app: a tour of `subms-timer-wheel`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features hierarchical`) to see the
//! feature sections light up.
//!
//! The framing is an order-lifecycle scheduler on a matching engine: every
//! resting order carries a time-in-force, and the wheel is what fires the
//! expiry when the clock reaches it.
//!
//! * base               - order time-in-force (TIF) expiry on the book
//! * hierarchical       - good-til-date orders whose horizons span seconds to a session
//! * concurrent         - quote-timeout timers armed from many market-data threads
//! * deadline-scheduler - session heartbeat against an absolute wall-clock deadline
//! * cron               - a recurring mark-to-market risk snapshot
//! * metrics            - per-instance scheduled/fired/cancelled/tick counters

use subms_timer_wheel::TimerWheel;

fn main() {
    base_tif_expiry();

    #[cfg(feature = "hierarchical")]
    hierarchical_gtd();

    #[cfg(feature = "concurrent")]
    concurrent_quote_timeouts();

    #[cfg(feature = "deadline-scheduler")]
    deadline_heartbeat();

    #[cfg(feature = "cron")]
    cron_risk_snapshot();

    #[cfg(feature = "metrics")]
    metered_expiry_wheel();
}

/// Base API: each resting order arms an expiry timer sized to its
/// time-in-force. If the order fills or is pulled first, the caller cancels
/// the timer; otherwise the tick that reaches its slot fires the expiry and
/// the order is swept off the book. One tick is one second of session time.
fn base_tif_expiry() {
    println!("== base: order time-in-force expiry ==");
    let mut expiries: TimerWheel<&'static str> = TimerWheel::new(256);

    let _ord_a = expiries.schedule(3, "ORD-A"); // 3s TIF
    let ord_b = expiries.schedule(5, "ORD-B"); // 5s TIF, but fills early
    let _ord_c = expiries.schedule(10, "ORD-C"); // 10s TIF
    println!("  armed 3 expiries, {} pending", expiries.pending());

    expiries.cancel(ord_b); // ORD-B fully filled at t=2, so cancel its expiry
    println!(
        "  ORD-B filled -> cancelled, {} pending",
        expiries.pending()
    );

    let mut expired = Vec::new();
    for second in 1..=10 {
        for id in expiries.tick() {
            println!("  t={second}s expire {id}");
            expired.push(id);
        }
    }
    println!("  -> expired {:?}", expired);

    assert_eq!(
        expired,
        vec!["ORD-A", "ORD-C"],
        "only uncancelled TIFs fire"
    );
    assert_eq!(expiries.pending(), 0, "every timer retired");
}

/// `hierarchical` feature: good-til-date orders expire anywhere from a few
/// seconds to a full session out. A single flat wheel would need a slot per
/// tick of the longest horizon; the hierarchical wheel holds far-out orders
/// on a coarse level and cascades them down as their deadline approaches, from
/// a fixed 192-bucket footprint.
#[cfg(feature = "hierarchical")]
fn hierarchical_gtd() {
    use subms_timer_wheel::HierarchicalTimerWheel;
    println!("\n== hierarchical: good-til-date across horizons ==");
    let mut gtd: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();

    gtd.schedule(30, "GTD-near"); // intraday, lands on the fine wheel
    gtd.schedule(300, "GTD-far"); // past 64 ticks, lands on a coarse wheel

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
    println!(
        "  near fired at t={:?}, far fired at t={:?}",
        near_at, far_at
    );
    println!("  cascade events: {}", gtd.cascades());

    assert_eq!(near_at, Some(30), "near GTD fires on its deadline");
    assert_eq!(far_at, Some(300), "far GTD fires on its deadline");
    assert!(gtd.cascades() >= 1, "the far order cascaded down a level");
}

/// `concurrent` feature: several market-data threads each arm quote-timeout
/// timers against one shared wheel. `Clone` shares the handle; every op
/// serializes on a short mutex. A single ticker thread then drains expiries.
#[cfg(feature = "concurrent")]
fn concurrent_quote_timeouts() {
    use std::thread;
    use subms_timer_wheel::ConcurrentTimerWheel;
    println!("\n== concurrent: quote timeouts from many feeds ==");
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
    println!("  {feeds} feeds x {per_feed} quotes -> {fired} timeouts fired");
    assert_eq!(
        fired,
        feeds * per_feed,
        "every armed timeout fired exactly once"
    );
}

/// `deadline-scheduler` feature: a session must emit a heartbeat by an absolute
/// wall-clock deadline. Callers think in instants; the layer maps them to ticks
/// through an injected clock, and `poll()` fires the catch-up batch. A hand-
/// stepped clock keeps the demo deterministic instead of sleeping. The
/// scheduler owns its clock, so we drive time through a shared handle.
#[cfg(feature = "deadline-scheduler")]
fn deadline_heartbeat() {
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
    let advance = |now: &Rc<Cell<u64>>, d: Duration| now.set(now.get() + d.as_nanos() as u64);

    println!("\n== deadline-scheduler: heartbeat by an absolute instant ==");
    let now = Rc::new(Cell::new(0u64));
    let mut sched =
        DeadlineScheduler::new(64, SharedClock(Rc::clone(&now)), Duration::from_millis(1));

    let deadline_nanos = Duration::from_millis(5).as_nanos() as u64;
    sched.schedule_at(deadline_nanos, "HEARTBEAT");

    advance(&now, Duration::from_millis(4));
    let early = sched.poll();
    println!("  at +4ms: {:?}", early);
    assert!(early.is_empty(), "nothing fires before the deadline");

    advance(&now, Duration::from_millis(1));
    let due = sched.poll();
    println!("  at +5ms: {:?}", due);
    assert_eq!(due, vec!["HEARTBEAT"], "heartbeat fires at its instant");
}

/// `cron` feature: a recurring risk snapshot on a wall-clock cadence. The
/// scheduler parses the 5-field expression once, then re-arms the next matching
/// second each time the current one fires.
#[cfg(feature = "cron")]
fn cron_risk_snapshot() {
    use subms_timer_wheel::{CronSchedule, CronScheduler};
    println!("\n== cron: mark-to-market every 5 minutes ==");
    let schedule = CronSchedule::parse("*/5 * * * *").expect("valid cron");

    // 2024-01-01 00:00:01 UTC. Next */5 boundary is 00:05:00.
    let start = 1_704_067_201;
    let mut scheduler = CronScheduler::new(schedule, start);

    let first = scheduler.next_fire(start).expect("a next fire exists");
    scheduler.record_fire(first);
    let second = scheduler.next_fire(first).expect("a next fire exists");
    println!("  first snapshot at epoch {first}, next at {second}");

    assert_eq!(
        first, 1_704_067_500,
        "first fire lands on the 5-minute grid"
    );
    assert_eq!(second, first + 300, "re-arms exactly 5 minutes later");
}

/// `metrics` feature: the expiry wheel reports its own cadence. The counters
/// are plain fields (the wheel is single-threaded), read through a snapshot.
#[cfg(feature = "metrics")]
fn metered_expiry_wheel() {
    use subms_timer_wheel::MeteredTimerWheel;
    println!("\n== metrics: self-reporting expiry counters ==");
    let mut wheel: MeteredTimerWheel<&'static str> = MeteredTimerWheel::new(64);

    let _a = wheel.schedule(2, "ORD-A");
    let b = wheel.schedule(2, "ORD-B");
    wheel.cancel(b);

    let mut fired = 0usize;
    for _ in 0..3 {
        fired += wheel.tick().len();
    }
    let m = wheel.metrics();
    println!(
        "  scheduled={} fired={} cancelled={} ticks={}",
        m.scheduled, m.fired, m.cancelled, m.ticks
    );

    assert_eq!(m.scheduled, 2);
    assert_eq!(m.cancelled, 1);
    assert_eq!(fired, 1, "only the uncancelled order fired");
    assert_eq!(m.fired, 1);
    assert_eq!(m.ticks, 3);
}
