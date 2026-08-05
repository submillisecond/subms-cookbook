use super::*;

#[test]
fn hit_and_miss_counts() {
    let mut c: MetricsCache<u32, u32> = MetricsCache::with_capacity(4);
    c.put(1, 10);
    let _ = c.get(&1);
    let _ = c.get(&1);
    let _ = c.get(&999);
    let _ = c.get(&42);
    assert_eq!(c.metrics().hits(), 2);
    assert_eq!(c.metrics().misses(), 2);
}

#[test]
fn evictions_increment_only_on_eviction() {
    let mut c: MetricsCache<u32, u32> = MetricsCache::with_capacity(2);
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.metrics().evictions(), 0);
    // Third put forces an eviction (clock-sweep base).
    c.put(3, 30);
    assert!(c.metrics().evictions() >= 1);
}

#[test]
fn admissions_count_each_put() {
    let mut c: MetricsCache<u32, u32> = MetricsCache::with_capacity(4);
    for k in 0u32..10 {
        c.put(k, k);
    }
    assert_eq!(c.metrics().admissions(), 10);
}

#[test]
fn hit_ratio_handles_zero() {
    let c: MetricsCache<u32, u32> = MetricsCache::with_capacity(4);
    assert_eq!(c.metrics().hit_ratio(), 0.0);
}

#[test]
fn hit_ratio_after_mixed_ops() {
    let mut c: MetricsCache<u32, u32> = MetricsCache::with_capacity(4);
    c.put(1, 10);
    let _ = c.get(&1);
    let _ = c.get(&1);
    let _ = c.get(&1);
    let _ = c.get(&99); // miss
    let r = c.metrics().hit_ratio();
    assert!((r - 0.75).abs() < 1e-9, "expected 0.75, got {r}");
}

#[test]
fn contention_counter_is_addressable() {
    let m = CacheMetrics::new();
    m.record_contention();
    m.record_contention();
    assert_eq!(m.contention_events(), 2);
}

#[test]
fn metrics_default_is_zero() {
    let m = CacheMetrics::default();
    assert_eq!(m.hits(), 0);
    assert_eq!(m.misses(), 0);
    assert_eq!(m.evictions(), 0);
    assert_eq!(m.admissions(), 0);
    assert_eq!(m.contention_events(), 0);
}

#[test]
fn cache_accessors_and_get_paths() {
    let mut c: MetricsCache<u32, u32> = MetricsCache::with_capacity(4);
    assert_eq!(c.capacity(), 4);
    assert!(c.is_empty());
    assert_eq!(c.len(), 0);
    c.put(1, 10);
    assert!(!c.is_empty());
    assert_eq!(c.len(), 1);
    assert_eq!(c.get(&1).copied(), Some(10)); // hit path returns the value
    assert!(c.get(&2).is_none()); // miss path returns None
    assert_eq!(c.metrics().hits(), 1);
    assert_eq!(c.metrics().misses(), 1);
    assert_eq!(c.metrics().admissions(), 1);
}

#[test]
fn remove_and_clear_delegate_without_moving_counters() {
    let mut c: MetricsCache<u32, u32> = MetricsCache::with_capacity(4);
    c.put(1, 10);
    c.put(2, 20);
    let evictions_before = c.metrics().evictions();
    assert_eq!(c.remove(&1), Some(10));
    assert_eq!(c.len(), 1);
    assert_eq!(
        c.metrics().evictions(),
        evictions_before,
        "invalidation is not eviction"
    );
    c.clear();
    assert!(c.is_empty());
    assert_eq!(c.metrics().admissions(), 2, "clear does not reset counters");
}
