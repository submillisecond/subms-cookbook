//! Per-key GCRA: one independent limiter per string key, in process.
//!
//! The base [`RateLimiter`](crate::RateLimiter) governs one flow. A gateway
//! usually has thousands - a quota per account, per symbol, per session - and
//! the state for each is the same single `u64` TAT. So the whole structure is a
//! concurrent map of keys to TATs, sharded so unrelated keys do not serialise
//! on one lock.
//!
//! Unlike the `distributed-backend` feature this is in-process and exact: it
//! keeps GCRA's smoothed outflow rather than dropping to fixed-window counters,
//! and there is no backend round trip. Use `distributed-backend` when the quota
//! has to span nodes.
//!
//! Memory is the thing to watch. A keyed limiter over unbounded keys (a client
//! id, an order id) grows without limit unless something sweeps it, which is
//! what [`KeyedRateLimiter::retain_active_at`] is for.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::Acquire;

const DEFAULT_SHARDS: usize = 16;

/// One GCRA limiter per key, all sharing the same rate and burst.
pub struct KeyedRateLimiter {
    shards: Box<[Mutex<HashMap<String, u64>>]>,
    period_ns: u64,
    burst_ns: u64,
    origin: Instant,
}

impl KeyedRateLimiter {
    /// `rate_per_sec` and `burst_capacity` apply to each key independently.
    pub fn new(rate_per_sec: f64, burst_capacity: u64) -> Self {
        Self::with_shards(rate_per_sec, burst_capacity, DEFAULT_SHARDS)
    }

    /// Shard count controls how much unrelated keys contend. Size it to the
    /// number of threads that will hit the limiter, not to the number of keys.
    pub fn with_shards(rate_per_sec: f64, burst_capacity: u64, shards: usize) -> Self {
        let period_ns = (1_000_000_000.0 / rate_per_sec) as u64;
        let burst_ns = period_ns.saturating_mul(burst_capacity.max(1));
        let shards = shards.max(1);
        Self {
            shards: (0..shards).map(|_| Mutex::new(HashMap::new())).collect(),
            period_ns,
            burst_ns,
            origin: Instant::now(),
        }
    }

    /// Try one permit on `key`.
    pub fn try_acquire(&self, key: &str) -> bool {
        self.try_acquire_n(key, 1)
    }

    /// Try `n` permits on `key`, all or nothing.
    pub fn try_acquire_n(&self, key: &str, n: u64) -> bool {
        matches!(self.try_acquire_at(self.now_ns(), key, n), Acquire::Ok)
    }

    /// Driven-time entry point. Returns the same typed outcome as the base
    /// limiter, so a rejected caller gets its retry-after for free.
    pub fn try_acquire_at(&self, now: u64, key: &str, n: u64) -> Acquire {
        if n == 0 {
            return Acquire::Ok;
        }
        let cost = self.period_ns.saturating_mul(n);
        if cost > self.burst_ns {
            return Acquire::Unattainable {
                burst_capacity: self.burst_capacity(),
            };
        }
        let mut shard = self.shard_for(key);
        let tat = shard.get(key).copied().unwrap_or(0);
        let new_tat = tat.max(now).saturating_add(cost);
        if new_tat.saturating_sub(now) > self.burst_ns {
            let wait = new_tat.saturating_sub(self.burst_ns).saturating_sub(now);
            return Acquire::Retry(Duration::from_nanos(wait));
        }
        // Both early returns above happen before the map is touched, so a probe
        // and an oversized request cost no memory.
        match shard.get_mut(key) {
            Some(slot) => *slot = new_tat,
            None => {
                shard.insert(key.to_string(), new_tat);
            }
        }
        Acquire::Ok
    }

    /// How long until `n` permits conform on `key`, without taking them.
    /// `None` when `n` exceeds the burst capacity.
    pub fn time_until_ready_at(&self, now: u64, key: &str, n: u64) -> Option<Duration> {
        if n == 0 {
            return Some(Duration::ZERO);
        }
        let cost = self.period_ns.saturating_mul(n);
        if cost > self.burst_ns {
            return None;
        }
        let shard = self.shard_for(key);
        let tat = shard.get(key).copied().unwrap_or(0);
        let new_tat = tat.max(now).saturating_add(cost);
        if new_tat.saturating_sub(now) > self.burst_ns {
            let wait = new_tat.saturating_sub(self.burst_ns).saturating_sub(now);
            Some(Duration::from_nanos(wait))
        } else {
            Some(Duration::ZERO)
        }
    }

    /// Drop `key`'s throttle state. Returns whether it was tracked.
    pub fn forget(&self, key: &str) -> bool {
        self.shard_for(key).remove(key).is_some()
    }

    /// Drop every key.
    pub fn clear(&self) {
        for s in self.shards.iter() {
            s.lock().unwrap_or_else(|e| e.into_inner()).clear();
        }
    }

    /// Evict keys whose TAT has fallen behind `now`, returning how many went.
    ///
    /// Eviction is lossless: a key at `tat <= now` has drawn nothing recently
    /// and its full burst is available, which is exactly the state a key that
    /// has never been seen starts in. Dropping it and re-admitting it later are
    /// the same limiter. Call it on a housekeeping tick to keep the map sized
    /// to the ACTIVE key set rather than the historical one.
    pub fn retain_active_at(&self, now: u64) -> usize {
        let mut evicted = 0;
        for s in self.shards.iter() {
            let mut m = s.lock().unwrap_or_else(|e| e.into_inner());
            let before = m.len();
            m.retain(|_, tat| *tat > now);
            evicted += before - m.len();
        }
        evicted
    }

    /// Number of keys currently carrying state.
    pub fn len(&self) -> usize {
        self.shards
            .iter()
            .map(|s| s.lock().unwrap_or_else(|e| e.into_inner()).len())
            .sum()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn now_ns(&self) -> u64 {
        self.origin.elapsed().as_nanos() as u64
    }

    pub fn rate_per_sec(&self) -> f64 {
        1_000_000_000.0 / self.period_ns as f64
    }

    pub fn burst_capacity(&self) -> u64 {
        self.burst_ns.checked_div(self.period_ns).unwrap_or(0)
    }

    pub fn shard_count(&self) -> usize {
        self.shards.len()
    }

    fn shard_for(&self, key: &str) -> std::sync::MutexGuard<'_, HashMap<String, u64>> {
        let idx = fnv1a(key) as usize % self.shards.len();
        self.shards[idx].lock().unwrap_or_else(|e| e.into_inner())
    }
}

/// FNV-1a over the key bytes. std's `DefaultHasher` is SipHash and randomly
/// seeded per process, which would make shard assignment differ run to run;
/// the shard index is not a security boundary and wants to be reproducible.
fn fnv1a(key: &str) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in key.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

#[cfg(test)]
#[path = "keyed_tests.rs"]
mod tests;
