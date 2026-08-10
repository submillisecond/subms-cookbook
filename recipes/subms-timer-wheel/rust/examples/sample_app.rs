//! Sample app: an order-lifecycle supervisor on a matching engine, built on
//! `subms-timer-wheel`. Run the base with `cargo run --example sample_app`;
//! add `--features full` to see each opt-in section light up.
//!
//! Every resting order arms an expiry timer sized to its time-in-force. A
//! fill cancels it, an amend reschedules it, and end-of-session drains what
//! is left. Session time is a tick counter and the clock-driven sections use
//! an injected clock, so the printed output is identical on every run.
//!
//! * base               - the TIF supervisor: arm, cancel on fill, amend, drain
//! * hierarchical       - good-til-date orders whose horizons span seconds to a session
//! * concurrent         - quote-timeout timers armed from many market-data threads
//! * deadline-scheduler - FIX session idle timeout, bumped by inbound traffic
//! * cron               - a recurring mark-to-market risk snapshot
//! * metrics            - the supervisor reporting its own cadence

use subms_timer_wheel::TimerWheel;

fn main() {
    tif_supervisor();

    #[cfg(feature = "hierarchical")]
    hierarchical_gtd();

    #[cfg(feature = "concurrent")]
    concurrent_quote_timeouts();

    #[cfg(feature = "deadline-scheduler")]
    deadline_session_idle();

    #[cfg(feature = "cron")]
    cron_risk_snapshot();

    #[cfg(feature = "metrics")]
    metered_expiry_wheel();
}

/// What the matching engine hands the supervisor, at a given session second.
enum Event {
    /// A new resting order with a time-in-force in seconds.
    Rest(&'static str, usize),
    /// Fully filled: its expiry must not fire.
    Fill(&'static str),
    /// Amended to a longer time-in-force, measured from now.
    Amend(&'static str, usize),
}

/// Base API. One tick is one second of session time. The supervisor holds an
/// order id to timer id map because the wheel hands back a timer id, and the
/// engine only ever speaks order ids.
fn tif_supervisor() {
    println!("== base: order time-in-force supervisor ==");

    let tape = [
        (0usize, Event::Rest("ORD-A", 3)),
        (0, Event::Rest("ORD-B", 5)),
        (0, Event::Rest("ORD-C", 9)),
        (0, Event::Rest("ORD-D", 12)),
        (2, Event::Fill("ORD-B")),
        (4, Event::Amend("ORD-C", 6)),
    ];

    let mut expiries: TimerWheel<&'static str> = TimerWheel::new(256);
    let mut timer_of: Vec<(&'static str, u64)> = Vec::new();
    let lookup = |map: &Vec<(&'static str, u64)>, ord: &str| {
        map.iter().find(|(o, _)| *o == ord).map(|(_, t)| *t)
    };

    let session_secs = 11;
    for second in 0..=session_secs {
        for (at, ev) in tape.iter() {
            if *at != second {
                continue;
            }
            match ev {
                Event::Rest(ord, tif) => {
                    let id = expiries.schedule(*tif, ord);
                    timer_of.push((ord, id));
                    println!("  t={second}s rest {ord} tif={tif}s");
                }
                Event::Fill(ord) => {
                    let id = lookup(&timer_of, ord).expect("a resting order");
                    expiries.cancel(id);
                    println!("  t={second}s fill {ord} -> expiry cancelled");
                }
                Event::Amend(ord, tif) => {
                    let id = lookup(&timer_of, ord).expect("a resting order");
                    expiries.reschedule(id, *tif);
                    println!("  t={second}s amend {ord} tif -> {tif}s from now");
                }
            }
        }
        if second == session_secs {
            break;
        }
        for ord in expiries.tick() {
            println!("  t={}s expire {ord}", second + 1);
        }
    }

    let unfilled = expiries.drain();
    println!(
        "  session close: {} orders still resting {:?}",
        unfilled.len(),
        unfilled
    );
    println!("  pending after drain: {}", expiries.pending());

    assert_eq!(
        unfilled,
        vec!["ORD-D"],
        "only the 12s TIF outlives the session"
    );
    assert_eq!(expiries.pending(), 0);
}

/// `hierarchical` feature: good-til-date orders expire anywhere from a few
/// seconds to a full session out. A single flat wheel would need a slot per
/// tick of the longest horizon; the hierarchical wheel holds far-out orders
/// on a coarse level and cascades them down as their deadline approaches,
/// from a fixed 192-bucket footprint.
#[cfg(feature = "hierarchical")]
fn hierarchical_gtd() {
    use subms_timer_wheel::HierarchicalTimerWheel;
    println!("\n== hierarchical: good-til-date across horizons ==");
    let mut gtd: HierarchicalTimerWheel<&'static str> = HierarchicalTimerWheel::new();

    gtd.schedule(30, "GTD-near"); // intraday, lands on the fine wheel
    let far = gtd.schedule(5000, "GTD-far"); // deep on the coarse wheel
    println!("  armed 2 GTD orders, {} pending", gtd.pending());

    // The desk pulls the far order in to the close of the current session.
    gtd.reschedule(far, 300);
    println!("  GTD-far pulled in to t=300");

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
    assert_eq!(
        far_at,
        Some(300),
        "the rescheduled GTD fires on its new deadline"
    );
    assert!(gtd.cascades() >= 1, "the far order cascaded down a level");
    assert_eq!(gtd.pending(), 0);
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
    println!(
        "  {} quote timeouts armed across {feeds} feeds",
        wheel.pending()
    );

    let fired = wheel.advance(16).len();
    println!("  {feeds} feeds x {per_feed} quotes -> {fired} timeouts fired");
    assert_eq!(
        fired,
        feeds * per_feed,
        "every armed timeout fired exactly once"
    );
    assert!(wheel.is_empty());
}

/// `deadline-scheduler` feature: a FIX session must see inbound traffic
/// inside its idle window or be torn down. One timer per session, bumped on
/// every message rather than cancelled and re-armed. Callers think in
/// instants; the layer maps them to ticks through the injected clock, and
/// `poll()` fires the catch-up batch. A hand-stepped clock keeps the demo
/// deterministic instead of sleeping.
#[cfg(feature = "deadline-scheduler")]
fn deadline_session_idle() {
    use std::time::Duration;
    use subms_timer_wheel::{DeadlineScheduler, TestClock};
    println!("\n== deadline-scheduler: FIX session idle timeout ==");

    let idle = Duration::from_millis(30);
    let mut sched: DeadlineScheduler<&'static str, TestClock> =
        DeadlineScheduler::new(256, TestClock::new(), Duration::from_millis(1));

    let session = sched.schedule_after(idle, "SESSION-1");
    let mut elapsed = 0u64;
    for gap in [10u64, 15] {
        sched.clock().advance(Duration::from_millis(gap));
        elapsed += gap;
        assert!(sched.poll().is_empty(), "traffic keeps the session alive");
        sched.reschedule_after(session, idle);
        println!(
            "  inbound msg at +{elapsed}ms, idle deadline now +{}ms",
            elapsed + 30
        );
    }
    // Then the counterparty goes quiet.
    sched.clock().advance(Duration::from_millis(30));
    let dead = sched.poll();
    println!("  no traffic for {}ms -> {:?}", idle.as_millis(), dead);
    assert_eq!(dead, vec!["SESSION-1"], "the idle timeout fires");
}

/// `cron` feature: a recurring risk snapshot on a wall-clock cadence. The
/// scheduler parses the 5-field expression once, then re-arms the next
/// matching second each time the current one fires.
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

/// `metrics` feature: the supervisor reports its own cadence. The counters
/// are plain fields (the wheel is single-threaded), read through a snapshot.
/// A drained timer is counted apart from a fired one, so a session close does
/// not read as a burst of expiries.
#[cfg(feature = "metrics")]
fn metered_expiry_wheel() {
    use subms_timer_wheel::MeteredTimerWheel;
    println!("\n== metrics: self-reporting expiry counters ==");
    let mut wheel: MeteredTimerWheel<&'static str> = MeteredTimerWheel::new(64);

    let a = wheel.schedule(2, "ORD-A");
    let b = wheel.schedule(2, "ORD-B");
    let c = wheel.schedule(2, "ORD-C");
    wheel.cancel(b);
    wheel.reschedule(c, 20);
    let _ = a;

    let fired = wheel.advance(3).len();
    let left = wheel.drain();
    let m = wheel.metrics();
    println!(
        "  scheduled={} fired={} cancelled={} rescheduled={} drained={} ticks={}",
        m.scheduled, m.fired, m.cancelled, m.rescheduled, m.drained, m.ticks
    );

    assert_eq!(m.scheduled, 3);
    assert_eq!(m.cancelled, 1);
    assert_eq!(m.rescheduled, 1);
    assert_eq!(fired, 1, "only the untouched order fired");
    assert_eq!(m.fired, 1);
    assert_eq!(left, vec!["ORD-C"]);
    assert_eq!(m.drained, 1);
    assert_eq!(m.ticks, 3);
}
