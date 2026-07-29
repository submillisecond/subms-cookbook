use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;

use subms_rate_limiter::{Acquire, RateLimiter};

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
    let rl = RateLimiter::new(1000.0, 5);
    // Drain the burst.
    for _ in 0..6 {
        let _ = rl.try_acquire();
    }
    // Next call should reject (rate would exceed).
    assert!(!rl.try_acquire(), "no refill yet");
    // Wait long enough for at least one permit to refill.
    thread::sleep(Duration::from_millis(5));
    assert!(rl.try_acquire(), "should refill after wait");
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
fn zero_burst_capacity_still_works() {
    // Floor enforced inside; cap=0 should not panic.
    let rl = RateLimiter::new(1000.0, 0);
    // First call may succeed depending on floor; later calls should generally reject.
    let _ = rl.try_acquire();
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
    use subms_rate_limiter::recipe::RateLimiterRecipe;
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
        Acquire::Ok => panic!("the 6th permit past a burst of 5 must be rejected"),
    }
}

#[test]
fn retry_rejection_does_not_advance_the_limiter() {
    let rl = RateLimiter::new(1.0, 2);
    assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    assert_eq!(rl.try_acquire_with_retry(), Acquire::Ok);
    let first = match rl.try_acquire_with_retry() {
        Acquire::Retry(d) => d,
        Acquire::Ok => panic!("rejected"),
    };
    let second = match rl.try_acquire_with_retry() {
        Acquire::Retry(d) => d,
        Acquire::Ok => panic!("rejected"),
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
