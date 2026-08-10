use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn allows_a_burst() {
    let rl = RateLimiter::new(100.0, 10);
    let mut got = 0;
    for _ in 0..20 {
        if rl.try_acquire() {
            got += 1;
        }
    }
    assert!(
        (10..=11).contains(&got),
        "expected ~burst permits, got {got}"
    );
}

#[test]
fn refills_over_time() {
    // Deterministic: drive `now` explicitly through the crate-internal
    // `try_acquire_at` rather than sleeping on the wall clock, so the
    // refill boundary is exact and the test never flakes under load.
    // 1000 permits/sec => period 1_000_000 ns; burst 5 => window 5_000_000 ns.
    let rl = RateLimiter::new(1000.0, 5);
    for _ in 0..5 {
        assert!(rl.try_acquire_at(0), "burst of 5 drains at t=0");
    }
    assert!(
        !rl.try_acquire_at(0),
        "6th permit at t=0 exceeds the burst window"
    );
    // Exactly one period of elapsed time slides one permit back into the
    // burst window; the rejected call above left `tat` untouched.
    assert!(
        rl.try_acquire_at(1_000_000),
        "one period elapsed refills one permit"
    );
    // Half a period more is not enough for another permit.
    assert!(
        !rl.try_acquire_at(1_500_000),
        "half a period does not refill a full permit"
    );
    assert!(
        rl.try_acquire_at(2_000_000),
        "second full period refills the next permit"
    );
}

#[test]
fn retry_after_arithmetic_is_deterministic() {
    // Drive `now` explicitly to pin the retry-after value without sleeps.
    let rl = RateLimiter::new(1000.0, 2); // period 1ms, burst window 2ms
    assert_eq!(rl.try_acquire_with_retry_at(0), Acquire::Ok);
    assert_eq!(rl.try_acquire_with_retry_at(0), Acquire::Ok);
    match rl.try_acquire_with_retry_at(0) {
        Acquire::Retry(d) => {
            // tat is at 2ms, new_tat would be 3ms; it must fall back inside
            // the 2ms window, i.e. wait 1ms from now=0.
            assert_eq!(d, Duration::from_nanos(1_000_000));
        }
        other => panic!("3rd permit past a burst of 2 must be rejected, got {other:?}"),
    }
    // After one period elapses, the same call conforms.
    assert_eq!(rl.try_acquire_with_retry_at(1_000_000), Acquire::Ok);
}

#[test]
fn rate_and_burst_accessors() {
    let rl = RateLimiter::new(2000.0, 8);
    assert!((rl.rate_per_sec() - 2000.0).abs() < 1.0);
    assert_eq!(rl.burst_capacity(), 8);
}

#[test]
fn concurrent_acquires_dont_double_spend() {
    let rl = Arc::new(RateLimiter::new(10_000.0, 100));
    let target = 100usize;
    let granted = Arc::new(AtomicUsize::new(0));
    let attempts_per_thread = 1000usize;

    let mut handles = Vec::new();
    for _ in 0..8 {
        let rl = rl.clone();
        let granted = granted.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..attempts_per_thread {
                if rl.try_acquire() {
                    granted.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // 8000 attempts in microseconds; at 10k/s + burst 100, far fewer than 8000
    // should be granted (most attempts happen within 1ms total).
    let g = granted.load(Ordering::Relaxed);
    assert!(g >= target, "burst should grant at least {target}, got {g}");
    assert!(g < 8000, "rate should reject most, got {g}");
}

#[test]
fn contended_acquires_exercise_the_cas_retry() {
    // Every thread passes the SAME `now` on the driven-time paths, so the only
    // thing that can separate them is the CAS. The burst is sized to the whole
    // workload so no attempt takes the early reject exit and every one races.
    let threads = 8usize;
    let attempts = 2000usize;
    let budget = (threads * attempts) as u64;
    let for_bool = Arc::new(RateLimiter::new(1_000_000.0, budget));
    let for_typed = Arc::new(RateLimiter::new(1_000_000.0, budget));
    // Elapsed time only ever adds permits, so a burst sized to the workload
    // makes the wall-clock count exact too.
    let for_wall = Arc::new(RateLimiter::new(1_000_000.0, budget));
    let counts = [
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
        Arc::new(AtomicUsize::new(0)),
    ];

    let mut handles = Vec::new();
    for _ in 0..threads {
        let (b, t, w) = (for_bool.clone(), for_typed.clone(), for_wall.clone());
        let c = counts.clone();
        handles.push(thread::spawn(move || {
            for _ in 0..attempts {
                if b.try_acquire_at(0) {
                    c[0].fetch_add(1, Ordering::Relaxed);
                }
                if t.try_acquire_n_with_retry_at(0, 1) == Acquire::Ok {
                    c[1].fetch_add(1, Ordering::Relaxed);
                }
                if w.try_acquire() {
                    c[2].fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }

    // A frozen clock means no refill: the burst is the exact budget, so a lost
    // CAS that granted anyway would show up as a count above it.
    for c in &counts {
        assert_eq!(c.load(Ordering::Relaxed) as u64, budget);
    }
    assert!(
        !for_bool.try_acquire_at(0),
        "the budget is spent to the permit"
    );
    assert!(
        !for_typed.try_acquire_at(0),
        "the budget is spent to the permit"
    );
}

#[test]
fn rejects_when_drained_immediately_after_burst() {
    let rl = RateLimiter::new(1.0, 1);
    assert!(rl.try_acquire());
    assert!(!rl.try_acquire());
    assert!(!rl.try_acquire());
}

#[test]
fn high_rate_low_burst_grants_steadily() {
    let rl = RateLimiter::new(100_000.0, 1);
    let mut got = 0;
    for _ in 0..1000 {
        if rl.try_acquire() {
            got += 1;
        }
    }
    // At 100k/s over ~microseconds, we should grant most or all.
    assert!(got >= 1, "burst should grant at least 1, got {got}");
}

#[test]
fn one_thread_full_steady_state() {
    let rl = RateLimiter::new(50_000.0, 100);
    let mut total = 0usize;
    for _ in 0..10_000 {
        if rl.try_acquire() {
            total += 1;
        }
    }
    assert!(total >= 100, "burst at minimum; got {total}");
}

#[test]
fn zero_burst_capacity_is_floored_to_one() {
    // A window of zero would reject the very first request. Both ports floor
    // the capacity at one permit instead.
    let rl = RateLimiter::new(1000.0, 0);
    assert_eq!(rl.burst_capacity(), 1);
    assert!(rl.try_acquire_at(0), "the floored window admits one permit");
    assert!(!rl.try_acquire_at(0), "and only one");
}

#[test]
fn weighted_draw_costs_n_periods() {
    // 1000/sec => period 1ms, burst 5 => window 5ms.
    let rl = RateLimiter::new(1000.0, 5);
    assert!(
        rl.try_acquire_n_at(0, 3),
        "a weight-3 message fits the window"
    );
    assert!(rl.try_acquire_n_at(0, 2), "the remaining 2 fit exactly");
    assert!(!rl.try_acquire_n_at(0, 1), "the window is spent");
    // Three periods of elapsed time buy back exactly three permits.
    assert!(rl.try_acquire_n_at(3_000_000, 3));
}

#[test]
fn a_rejected_weighted_draw_spends_nothing() {
    let rl = RateLimiter::new(1000.0, 5);
    assert!(rl.try_acquire_n_at(0, 4));
    assert!(!rl.try_acquire_n_at(0, 3), "4 + 3 overshoots a burst of 5");
    assert!(
        rl.try_acquire_n_at(0, 1),
        "the rejected batch must not have spent the remaining permit"
    );
}

#[test]
fn weight_above_burst_is_typed_as_unattainable() {
    let rl = RateLimiter::new(1000.0, 5);
    assert_eq!(
        rl.try_acquire_n_with_retry_at(0, 6),
        Acquire::Unattainable { burst_capacity: 5 },
        "no wait can satisfy a weight above the burst window"
    );
    assert_eq!(rl.time_until_ready_at(0, 6), None);
    assert!(rl.try_acquire_n_at(0, 5), "the limiter is untouched by it");
}

#[test]
fn zero_weight_is_a_free_probe() {
    let rl = RateLimiter::new(1000.0, 2);
    assert_eq!(rl.try_acquire_n_with_retry_at(0, 0), Acquire::Ok);
    assert_eq!(rl.time_until_ready_at(0, 0), Some(Duration::ZERO));
    assert!(rl.try_acquire_n_at(0, 2), "the probe advanced nothing");
}

#[test]
fn time_until_ready_reads_without_spending() {
    let rl = RateLimiter::new(1000.0, 2); // period 1ms, window 2ms
    assert_eq!(rl.time_until_ready_at(0, 1), Some(Duration::ZERO));
    assert_eq!(
        rl.time_until_ready_at(0, 1),
        Some(Duration::ZERO),
        "a peek must not consume the permit it reports on"
    );
    assert!(rl.try_acquire_n_at(0, 2));
    // Saturated: the next permit conforms one period from now.
    assert_eq!(
        rl.time_until_ready_at(0, 1),
        Some(Duration::from_nanos(1_000_000))
    );
    // And the peek agrees with what the mutating call reports.
    match rl.try_acquire_with_retry_at(0) {
        Acquire::Retry(d) => assert_eq!(d, Duration::from_nanos(1_000_000)),
        other => panic!("expected a rejection, got {other:?}"),
    }
    assert_eq!(rl.time_until_ready_at(1_000_000, 1), Some(Duration::ZERO));
}

#[test]
fn reset_returns_the_full_burst() {
    let rl = RateLimiter::new(1000.0, 3);
    for _ in 0..3 {
        assert!(rl.try_acquire_at(0));
    }
    assert!(!rl.try_acquire_at(0));
    rl.reset();
    for _ in 0..3 {
        assert!(rl.try_acquire_at(0), "reset restores the whole burst");
    }
    assert!(!rl.try_acquire_at(0));
}

#[test]
fn acquire_within_returns_immediately_when_permitted() {
    let rl = RateLimiter::new(1000.0, 4);
    let started = std::time::Instant::now();
    assert!(rl.acquire_within(1, Duration::from_secs(5)));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "an available permit must not sleep"
    );
}

#[test]
fn acquire_within_gives_up_rather_than_sleeping_past_the_deadline() {
    // period 1s, burst 1: the second permit is a second away.
    let rl = RateLimiter::new(1.0, 1);
    assert!(rl.acquire_within(1, Duration::from_millis(1)));
    let started = std::time::Instant::now();
    assert!(
        !rl.acquire_within(1, Duration::from_millis(1)),
        "a 1s wait cannot be satisfied inside a 1ms timeout"
    );
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "it must refuse without sleeping"
    );
    // An unattainable weight is refused on the spot, not waited on.
    assert!(!rl.acquire_within(9, Duration::from_secs(30)));
}

#[test]
fn acquire_within_sleeps_then_succeeds() {
    // period 2ms, burst 1. The first call takes the permit; the second has to
    // wait ~2ms, comfortably inside the 5s timeout even on a loaded box.
    let rl = RateLimiter::new(500.0, 1);
    assert!(rl.acquire_within(1, Duration::from_secs(5)));
    assert!(rl.acquire_within(1, Duration::from_secs(5)));
}

#[test]
fn now_ns_tracks_the_limiters_own_origin() {
    let rl = RateLimiter::new(1000.0, 4);
    let a = rl.now_ns();
    let b = rl.now_ns();
    assert!(b >= a, "the clock is monotonic");
    assert!(a < 1_000_000_000, "and starts at the limiter's origin");
    // The driven-time API and the internal clock agree on the same limiter.
    assert!(rl.try_acquire_at(rl.now_ns()));
}

#[test]
fn very_high_rate_does_not_overflow() {
    // 1e9 permits/sec corresponds to period_ns = 1.
    let rl = RateLimiter::new(1_000_000_000.0, 1000);
    assert_eq!(rl.burst_capacity(), 1000);
    for _ in 0..100 {
        assert!(rl.try_acquire());
    }
}

#[cfg(feature = "harness")]
mod _harness_compile_check {
    use crate::recipe::RateLimiterRecipe;
    #[test]
    fn recipe_is_constructable() {
        let _r = RateLimiterRecipe;
    }
}

#[test]
fn retry_reports_ok_while_under_limit() {
    let rl = RateLimiter::new(1.0, 5);
    for _ in 0..5 {
        assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    }
}

#[test]
fn retry_reports_wait_when_exhausted() {
    let rl = RateLimiter::new(1.0, 5); // period 1s, burst 5
    for _ in 0..5 {
        assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    }
    match rl.try_acquire_with_retry() {
        Acquire::Retry(d) => {
            assert!(
                d.as_nanos() > 0,
                "a rejected request must wait a positive time"
            );
            // Wait is bounded by one token period (1ms); elapsed real time shrinks it.
            assert!(
                d <= Duration::from_secs(1),
                "wait bounded by the token period, got {d:?}"
            );
        }
        other => panic!("the 6th permit past a burst of 5 must be rejected, got {other:?}"),
    }
}

#[test]
fn retry_rejection_does_not_advance_the_limiter() {
    let rl = RateLimiter::new(1.0, 2);
    assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    let first = match rl.try_acquire_with_retry() {
        Acquire::Retry(d) => d,
        other => panic!("expected a rejection, got {other:?}"),
    };
    let second = match rl.try_acquire_with_retry() {
        Acquire::Retry(d) => d,
        other => panic!("expected a rejection, got {other:?}"),
    };
    // A rejection leaves `tat` untouched, so the wait never grows across repeated
    // rejections - only elapsed real time shrinks it.
    assert!(
        second <= first,
        "rejection must not advance tat: {first:?} then {second:?}"
    );
}

#[test]
fn retry_ok_agrees_with_try_acquire() {
    let rl = RateLimiter::new(1.0, 3);
    assert!(rl.try_acquire());
    assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    assert!(rl.try_acquire());
    assert!(!rl.try_acquire(), "burst of 3 is exhausted");
    assert!(matches!(rl.try_acquire_with_retry(), Acquire::Retry(_)));
}
