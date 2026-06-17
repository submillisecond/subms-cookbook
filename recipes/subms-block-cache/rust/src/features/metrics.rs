//! Per-instance counters: hits, misses, evictions, admissions, and
//! shard-contention events.
//!
//! Wraps the base `BlockCache` and adds atomic counters around `get` /
//! `put`. Use this when you want a single-instance summary; for system-
//! wide histograms reach for `subms-hdr-histogram`.
//!
//! All counters are `AtomicU64` so the cache is `Send + Sync` and can
//! be wrapped in `Arc` for multi-threaded summary reads even when the
//! cache itself is single-writer.

use std::hash::Hash;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::BlockCache;

#[derive(Debug, Default)]
pub struct CacheMetrics {
    pub hits: AtomicU64,
    pub misses: AtomicU64,
    pub evictions: AtomicU64,
    pub admissions: AtomicU64,
    pub contention_events: AtomicU64,
}

impl CacheMetrics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn hits(&self) -> u64 {
        self.hits.load(Ordering::Relaxed)
    }
    pub fn misses(&self) -> u64 {
        self.misses.load(Ordering::Relaxed)
    }
    pub fn evictions(&self) -> u64 {
        self.evictions.load(Ordering::Relaxed)
    }
    pub fn admissions(&self) -> u64 {
        self.admissions.load(Ordering::Relaxed)
    }
    pub fn contention_events(&self) -> u64 {
        self.contention_events.load(Ordering::Relaxed)
    }

    pub fn hit_ratio(&self) -> f64 {
        let h = self.hits() as f64;
        let m = self.misses() as f64;
        let total = h + m;
        if total == 0.0 { 0.0 } else { h / total }
    }

    pub fn record_hit(&self) {
        self.hits.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_miss(&self) {
        self.misses.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_eviction(&self) {
        self.evictions.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_admission(&self) {
        self.admissions.fetch_add(1, Ordering::Relaxed);
    }
    pub fn record_contention(&self) {
        self.contention_events.fetch_add(1, Ordering::Relaxed);
    }
}

/// Counter-wrapped block cache.
pub struct MetricsCache<K, V> {
    inner: BlockCache<K, V>,
    metrics: CacheMetrics,
}

impl<K: Hash + Eq + Clone, V> MetricsCache<K, V> {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            inner: BlockCache::with_capacity(capacity),
            metrics: CacheMetrics::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
    pub fn len(&self) -> usize {
        self.inner.len()
    }
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }
    pub fn metrics(&self) -> &CacheMetrics {
        &self.metrics
    }

    pub fn get(&mut self, key: &K) -> Option<&V> {
        let r = self.inner.get(key);
        if r.is_some() {
            self.metrics.record_hit();
        } else {
            self.metrics.record_miss();
        }
        r
    }

    pub fn put(&mut self, key: K, value: V) -> Option<(K, V)> {
        let r = self.inner.put(key, value);
        self.metrics.record_admission();
        if r.is_some() {
            self.metrics.record_eviction();
        }
        r
    }
}

#[cfg(test)]
mod tests {
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
}
