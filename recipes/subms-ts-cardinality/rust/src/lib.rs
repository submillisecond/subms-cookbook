//! `subms-ts-cardinality` - admission guards for a multi-tenant
//! [`TsCollection`]. Three composable primitives, each O(1) on the hot path
//! and each holding only counters or a key set of its own; none of them
//! touch the subms-ts core.
//!
//! - [`TsCardinalityGuard`] caps the number of live series and decides admit
//!   vs reject under a [`TsOverflowPolicy`].
//! - [`TsTenantedGuard`] applies an independent per-tenant cap, so one noisy
//!   tenant cannot starve the others.
//! - [`TsDedupFilter`] makes ingest idempotent: a replayed
//!   [`TsIngestKey`] is recognised and dropped.
//!
//! [`TsGuardedCollection`] wires the series-count guard onto a real
//! `TsCollection` so `register` enforces the cap and reads delegate straight
//! through.
//!
//! ```
//! use subms_ts_cardinality::{TsCardinalityGuard, TsOverflowPolicy};
//!
//! let mut guard = TsCardinalityGuard::new(2, TsOverflowPolicy::Reject);
//! assert!(guard.admit().is_ok());
//! assert!(guard.admit().is_ok());
//! assert!(guard.admit().is_err()); // at the cap
//! guard.release();
//! assert!(guard.admit().is_ok()); // a slot opened up
//! ```

use std::collections::{HashMap, HashSet};

use subms_ts::{TsCollection, TsSeriesMetadata};

#[cfg(feature = "harness")]
pub mod recipe;

/// What a guard does once its cap is reached.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TsOverflowPolicy {
    /// Refuse the admission: `admit` returns a [`TsCardinalityError`].
    #[default]
    Reject,
    /// Admit anyway and keep counting. The cap becomes a soft watermark you
    /// can read back via [`TsCardinalityGuard::over_count`].
    Allow,
}

/// Admission failure. The cap that bound and (for the tenanted case) which
/// tenant tripped it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TsCardinalityError {
    /// The global series-count cap was reached under [`TsOverflowPolicy::Reject`].
    CardinalityCap { max: usize },
    /// A single tenant's cap was reached under [`TsOverflowPolicy::Reject`].
    TenantCardinalityCap { tenant: u64, max: usize },
}

impl std::fmt::Display for TsCardinalityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TsCardinalityError::CardinalityCap { max } => {
                write!(f, "series cardinality cap reached (max {max})")
            }
            TsCardinalityError::TenantCardinalityCap { tenant, max } => {
                write!(f, "tenant {tenant} cardinality cap reached (max {max})")
            }
        }
    }
}

impl std::error::Error for TsCardinalityError {}

/// A series-count cap. Tracks its own admitted count and decides admit vs
/// reject. Pure counter arithmetic - the guard never sees the series itself.
#[derive(Clone, Debug)]
pub struct TsCardinalityGuard {
    max: usize,
    policy: TsOverflowPolicy,
    count: usize,
}

impl TsCardinalityGuard {
    pub fn new(max_series: usize, policy: TsOverflowPolicy) -> Self {
        Self {
            max: max_series,
            policy,
            count: 0,
        }
    }

    /// Try to admit one series. Increments the count when admitted. Under
    /// [`TsOverflowPolicy::Reject`] a full guard returns
    /// [`TsCardinalityError::CardinalityCap`] and leaves the count unchanged;
    /// under [`TsOverflowPolicy::Allow`] it always admits and the count is
    /// allowed to climb past `max`.
    pub fn admit(&mut self) -> Result<(), TsCardinalityError> {
        if self.count >= self.max && self.policy == TsOverflowPolicy::Reject {
            return Err(TsCardinalityError::CardinalityCap { max: self.max });
        }
        self.count += 1;
        Ok(())
    }

    /// Release one admitted slot. Saturates at zero so a stray release can
    /// never underflow the count.
    pub fn release(&mut self) {
        self.count = self.count.saturating_sub(1);
    }

    pub fn count(&self) -> usize {
        self.count
    }

    pub fn max(&self) -> usize {
        self.max
    }

    /// Free slots before the cap binds. Zero once the count reaches `max`,
    /// including the over-admitted [`TsOverflowPolicy::Allow`] case.
    pub fn remaining(&self) -> usize {
        self.max.saturating_sub(self.count)
    }

    /// How far the count has climbed past `max`. Always zero under
    /// [`TsOverflowPolicy::Reject`]; non-zero only when `Allow` admitted over
    /// the cap.
    pub fn over_count(&self) -> usize {
        self.count.saturating_sub(self.max)
    }

    /// True when the next [`TsOverflowPolicy::Reject`] admit would fail (or,
    /// under `Allow`, would push the count past `max`).
    pub fn would_exceed(&self) -> bool {
        self.count >= self.max
    }
}

/// A tenant handle. A thin newtype so a tenant id never gets confused with a
/// series id or a count.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TsTenantId(pub u64);

/// Per-tenant cardinality. Each tenant gets the same `max_per_tenant` cap,
/// tracked independently, so a tenant at its limit never blocks another.
#[derive(Clone, Debug)]
pub struct TsTenantedGuard {
    max_per_tenant: usize,
    policy: TsOverflowPolicy,
    counts: HashMap<u64, usize>,
}

impl TsTenantedGuard {
    pub fn new(max_per_tenant: usize, policy: TsOverflowPolicy) -> Self {
        Self {
            max_per_tenant,
            policy,
            counts: HashMap::new(),
        }
    }

    /// Try to admit one series for `tenant`. Under [`TsOverflowPolicy::Reject`]
    /// a tenant at its cap returns [`TsCardinalityError::TenantCardinalityCap`]
    /// and is left unchanged; under [`TsOverflowPolicy::Allow`] the tenant's
    /// count is allowed to climb past the cap.
    pub fn admit(&mut self, tenant: TsTenantId) -> Result<(), TsCardinalityError> {
        let entry = self.counts.entry(tenant.0).or_insert(0);
        if *entry >= self.max_per_tenant && self.policy == TsOverflowPolicy::Reject {
            return Err(TsCardinalityError::TenantCardinalityCap {
                tenant: tenant.0,
                max: self.max_per_tenant,
            });
        }
        *entry += 1;
        Ok(())
    }

    /// Release one slot for `tenant`. Saturates at zero.
    pub fn release(&mut self, tenant: TsTenantId) {
        if let Some(c) = self.counts.get_mut(&tenant.0) {
            *c = c.saturating_sub(1);
        }
    }

    pub fn count(&self, tenant: TsTenantId) -> usize {
        self.counts.get(&tenant.0).copied().unwrap_or(0)
    }

    pub fn remaining(&self, tenant: TsTenantId) -> usize {
        self.max_per_tenant.saturating_sub(self.count(tenant))
    }

    pub fn max_per_tenant(&self) -> usize {
        self.max_per_tenant
    }

    /// Tenants seen at least once (admitted or attempted). Order is arbitrary.
    pub fn tenants(&self) -> impl Iterator<Item = TsTenantId> + '_ {
        self.counts.keys().copied().map(TsTenantId)
    }

    pub fn tenant_count(&self) -> usize {
        self.counts.len()
    }
}

/// Identity of one ingested point for dedup purposes: which series and the
/// caller-assigned monotonic sequence within it. A replayed `(series_id,
/// sequence)` is the same logical write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TsIngestKey {
    pub series_id: u64,
    pub sequence: u64,
}

impl TsIngestKey {
    pub fn new(series_id: u64, sequence: u64) -> Self {
        Self {
            series_id,
            sequence,
        }
    }
}

/// Exact idempotent-ingest filter. The first sight of a [`TsIngestKey`] is new;
/// every later sight of the same key is a replay and is dropped.
///
/// Backed by an exact `HashSet`, so memory grows with the number of distinct
/// keys seen. That is the right call when the dedup window is bounded
/// upstream (a fixed sequence range per series, a per-flush reset). For an
/// unbounded stream a future bounded / rolling-bloom variant trades exactness
/// for a fixed footprint - see the recipe writeup.
#[derive(Clone, Debug, Default)]
pub struct TsDedupFilter {
    seen: HashSet<(u64, u64)>,
}

impl TsDedupFilter {
    pub fn new() -> Self {
        Self {
            seen: HashSet::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        Self {
            seen: HashSet::with_capacity(cap),
        }
    }

    /// True the first time `key` is seen (and records it); false on any replay.
    pub fn is_new(&mut self, key: TsIngestKey) -> bool {
        self.seen.insert((key.series_id, key.sequence))
    }

    /// Test without recording. Lets a caller peek before committing the write.
    pub fn contains(&self, key: TsIngestKey) -> bool {
        self.seen.contains(&(key.series_id, key.sequence))
    }

    pub fn seen_count(&self) -> usize {
        self.seen.len()
    }

    pub fn is_empty(&self) -> bool {
        self.seen.is_empty()
    }

    /// Forget every key. Use at a dedup-window boundary (a flush, a new epoch)
    /// to reclaim the set's memory.
    pub fn reset(&mut self) {
        self.seen.clear();
    }
}

/// A `TsCollection` with a series-count cap bolted on. `register` consults the
/// guard before delegating; every read passes straight through. The decorator
/// owns both halves so the guard's count and the collection's series stay in
/// lockstep.
#[derive(Debug)]
pub struct TsGuardedCollection<T> {
    inner: TsCollection<T>,
    guard: TsCardinalityGuard,
}

impl<T: Clone> TsGuardedCollection<T> {
    pub fn new(max_series: usize, policy: TsOverflowPolicy) -> Self {
        Self {
            inner: TsCollection::new(),
            guard: TsCardinalityGuard::new(max_series, policy),
        }
    }

    /// Register a series if the cap allows. The guard decides first; only an
    /// accepted admission reaches the collection. A duplicate id / name still
    /// fails inside `TsCollection`, in which case the borrowed guard slot is
    /// released so a rejected register does not silently consume capacity.
    pub fn register(&mut self, meta: TsSeriesMetadata) -> Result<u64, TsCardinalityError> {
        self.guard.admit()?;
        match self.inner.register(meta) {
            Ok(id) => Ok(id),
            Err(_) => {
                self.guard.release();
                // Re-run admit only to surface the cap error shape if the
                // guard is now full; otherwise report the cap that bounds.
                Err(TsCardinalityError::CardinalityCap {
                    max: self.guard.max(),
                })
            }
        }
    }

    /// Push a point into a registered series. No cardinality decision: a push
    /// adds to an existing series, it does not create one.
    pub fn push(&mut self, id: u64, ts: i64, value: T) -> bool
    where
        T: subms_ts::TsValueKind,
    {
        self.inner.push(id, ts, value).is_ok()
    }

    pub fn get(&self, id: u64) -> Option<&subms_ts::TsSeries<T>> {
        self.inner.get(id)
    }

    pub fn by_name(&self, name: &str) -> Option<&subms_ts::TsSeries<T>> {
        self.inner.by_name(name)
    }

    /// Deregister a series and free its guard slot.
    pub fn deregister(&mut self, id: u64) -> Option<subms_ts::TsSeries<T>> {
        let removed = self.inner.deregister(id);
        if removed.is_some() {
            self.guard.release();
        }
        removed
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    pub fn remaining(&self) -> usize {
        self.guard.remaining()
    }

    pub fn count(&self) -> usize {
        self.guard.count()
    }

    /// Borrow the wrapped collection for read-only operations not surfaced on
    /// the decorator (tag scans, aggregation).
    pub fn collection(&self) -> &TsCollection<T> {
        &self.inner
    }
}
