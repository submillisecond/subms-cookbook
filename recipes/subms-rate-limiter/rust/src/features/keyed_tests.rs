use super::*;

/// 1000/sec => period 1ms; burst 3 => window 3ms.
fn limiter() -> KeyedRateLimiter {
    KeyedRateLimiter::new(1000.0, 3)
}

#[test]
fn keys_are_independent() {
    let k = limiter();
    for _ in 0..3 {
        assert_eq!(k.try_acquire_at(0, "acct-a", 1), Acquire::Ok);
    }
    assert!(matches!(
        k.try_acquire_at(0, "acct-a", 1),
        Acquire::Retry(_)
    ));
    // A saturated key must not throttle its neighbour.
    assert_eq!(k.try_acquire_at(0, "acct-b", 1), Acquire::Ok);
}

#[test]
fn per_key_refill_follows_the_clock() {
    let k = limiter();
    for _ in 0..3 {
        assert_eq!(k.try_acquire_at(0, "sym-ESU5", 1), Acquire::Ok);
    }
    assert!(matches!(
        k.try_acquire_at(0, "sym-ESU5", 1),
        Acquire::Retry(_)
    ));
    assert_eq!(k.try_acquire_at(1_000_000, "sym-ESU5", 1), Acquire::Ok);
    assert!(matches!(
        k.try_acquire_at(1_500_000, "sym-ESU5", 1),
        Acquire::Retry(_)
    ));
}

#[test]
fn retry_after_matches_the_base_limiter() {
    let k = limiter();
    for _ in 0..3 {
        k.try_acquire_at(0, "acct-a", 1);
    }
    match k.try_acquire_at(0, "acct-a", 1) {
        Acquire::Retry(d) => assert_eq!(d, Duration::from_nanos(1_000_000)),
        other => panic!("expected a retry-after, got {other:?}"),
    }
}

#[test]
fn weighted_draw_costs_n_periods() {
    let k = limiter();
    assert_eq!(k.try_acquire_at(0, "acct-a", 3), Acquire::Ok);
    assert!(matches!(
        k.try_acquire_at(0, "acct-a", 1),
        Acquire::Retry(_)
    ));
}

#[test]
fn weight_above_burst_is_unattainable() {
    let k = limiter();
    assert_eq!(
        k.try_acquire_at(0, "acct-a", 4),
        Acquire::Unattainable { burst_capacity: 3 }
    );
    // An unattainable request must not have left state behind.
    assert!(k.is_empty());
}

#[test]
fn zero_weight_is_a_free_probe() {
    let k = limiter();
    assert_eq!(k.try_acquire_at(0, "acct-a", 0), Acquire::Ok);
    assert!(k.is_empty(), "a zero-weight probe tracks no state");
}

#[test]
fn tracked_key_count_follows_granted_keys() {
    let k = limiter();
    for _ in 0..3 {
        k.try_acquire_at(0, "hot", 1);
    }
    k.try_acquire_at(0, "warm", 1);
    assert_eq!(k.len(), 2);
    // An oversized request is answered before the map is touched, so a caller
    // cannot grow the key set with requests that can never be granted.
    assert!(matches!(
        k.try_acquire_at(0, "never-granted", 9),
        Acquire::Unattainable { .. }
    ));
    assert_eq!(k.len(), 2);
}

#[test]
fn time_until_ready_does_not_spend() {
    let k = limiter();
    assert_eq!(k.time_until_ready_at(0, "acct-a", 3), Some(Duration::ZERO));
    // Peeking three times in a row must still report ready.
    assert_eq!(k.time_until_ready_at(0, "acct-a", 3), Some(Duration::ZERO));
    assert!(k.is_empty());
    assert_eq!(k.try_acquire_at(0, "acct-a", 3), Acquire::Ok);
    assert_eq!(
        k.time_until_ready_at(0, "acct-a", 1),
        Some(Duration::from_nanos(1_000_000))
    );
    assert_eq!(k.time_until_ready_at(0, "acct-a", 9), None);
}

#[test]
fn retain_active_evicts_idle_keys_only() {
    let k = limiter();
    k.try_acquire_at(0, "idle", 1); // tat = 1ms
    k.try_acquire_at(5_000_000, "busy", 3); // tat = 8ms
    assert_eq!(k.len(), 2);

    assert_eq!(k.retain_active_at(5_000_000), 1, "the idle key goes");
    assert_eq!(k.len(), 1);

    // Eviction is lossless: the evicted key comes back at full burst, which is
    // what it had anyway after 5ms of idling.
    for _ in 0..3 {
        assert_eq!(k.try_acquire_at(5_000_000, "idle", 1), Acquire::Ok);
    }
}

#[test]
fn forget_and_clear_drop_state() {
    let k = limiter();
    k.try_acquire_at(0, "a", 3);
    k.try_acquire_at(0, "b", 3);
    assert!(k.forget("a"));
    assert!(!k.forget("a"), "forgetting twice is not an error");
    assert_eq!(k.len(), 1);
    assert_eq!(k.try_acquire_at(0, "a", 3), Acquire::Ok, "reset by forget");
    k.clear();
    assert!(k.is_empty());
}

#[test]
fn config_accessors_round_trip() {
    let k = KeyedRateLimiter::with_shards(2000.0, 8, 4);
    assert!((k.rate_per_sec() - 2000.0).abs() < 1.0);
    assert_eq!(k.burst_capacity(), 8);
    assert_eq!(k.shard_count(), 4);
    assert_eq!(KeyedRateLimiter::with_shards(1000.0, 1, 0).shard_count(), 1);
    assert!(k.now_ns() < 1_000_000_000, "clock starts at the origin");
}

#[test]
fn concurrent_keys_do_not_double_spend() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    // 10/sec means one refill per 100 ms, so nothing refills inside the run and
    // the expected total is exactly 4 keys x a burst of 50.
    let k = Arc::new(KeyedRateLimiter::new(10.0, 50));
    let granted = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for _ in 0..8 {
        let k = k.clone();
        let granted = granted.clone();
        handles.push(thread::spawn(move || {
            for i in 0..500 {
                if k.try_acquire(&format!("key-{}", i % 4)) {
                    granted.fetch_add(1, Ordering::Relaxed);
                }
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let g = granted.load(Ordering::Relaxed);
    assert!(
        g >= 200,
        "4 keys x burst 50 should all be spendable, got {g}"
    );
    assert!(
        g <= 210,
        "8 threads must not double-spend 4 bursts, got {g}"
    );
    assert_eq!(k.len(), 4);
}
