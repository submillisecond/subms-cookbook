use subms_ts::TsSeriesMetadata;
use subms_ts_cardinality::{
    TsCardinalityError, TsCardinalityGuard, TsDedupFilter, TsGuardedCollection, TsIngestKey,
    TsOverflowPolicy, TsTenantId, TsTenantedGuard,
};

// ---------- TsCardinalityGuard ----------

#[test]
fn guard_admits_up_to_max_then_rejects() {
    let mut g = TsCardinalityGuard::new(3, TsOverflowPolicy::Reject);
    assert!(g.admit().is_ok());
    assert!(g.admit().is_ok());
    assert!(g.admit().is_ok());
    assert_eq!(g.count(), 3);
    assert_eq!(
        g.admit(),
        Err(TsCardinalityError::CardinalityCap { max: 3 })
    );
    // a rejected admit leaves the count untouched.
    assert_eq!(g.count(), 3);
}

#[test]
fn guard_allow_policy_admits_past_cap() {
    let mut g = TsCardinalityGuard::new(2, TsOverflowPolicy::Allow);
    for _ in 0..5 {
        assert!(g.admit().is_ok());
    }
    assert_eq!(g.count(), 5);
    assert_eq!(g.over_count(), 3);
    assert_eq!(g.remaining(), 0);
    assert!(g.would_exceed());
}

#[test]
fn guard_release_frees_a_slot() {
    let mut g = TsCardinalityGuard::new(1, TsOverflowPolicy::Reject);
    assert!(g.admit().is_ok());
    assert!(g.admit().is_err());
    g.release();
    assert_eq!(g.count(), 0);
    assert!(g.admit().is_ok());
}

#[test]
fn guard_release_saturates_at_zero() {
    let mut g = TsCardinalityGuard::new(4, TsOverflowPolicy::Reject);
    g.release();
    g.release();
    assert_eq!(g.count(), 0);
}

#[test]
fn guard_remaining_and_would_exceed() {
    let mut g = TsCardinalityGuard::new(2, TsOverflowPolicy::Reject);
    assert_eq!(g.remaining(), 2);
    assert!(!g.would_exceed());
    g.admit().unwrap();
    assert_eq!(g.remaining(), 1);
    assert!(!g.would_exceed());
    g.admit().unwrap();
    assert_eq!(g.remaining(), 0);
    assert!(g.would_exceed());
    assert_eq!(g.over_count(), 0); // reject never over-admits
}

// ---------- TsTenantedGuard ----------

#[test]
fn tenanted_caps_each_tenant_independently() {
    let mut g = TsTenantedGuard::new(2, TsOverflowPolicy::Reject);
    let a = TsTenantId(1);
    let b = TsTenantId(2);
    assert!(g.admit(a).is_ok());
    assert!(g.admit(a).is_ok());
    // tenant A is full ...
    assert_eq!(
        g.admit(a),
        Err(TsCardinalityError::TenantCardinalityCap { tenant: 1, max: 2 })
    );
    // ... which does not block tenant B.
    assert!(g.admit(b).is_ok());
    assert!(g.admit(b).is_ok());
    assert_eq!(g.count(a), 2);
    assert_eq!(g.count(b), 2);
}

#[test]
fn tenanted_count_and_tenants() {
    let mut g = TsTenantedGuard::new(5, TsOverflowPolicy::Reject);
    g.admit(TsTenantId(10)).unwrap();
    g.admit(TsTenantId(10)).unwrap();
    g.admit(TsTenantId(20)).unwrap();
    assert_eq!(g.count(TsTenantId(10)), 2);
    assert_eq!(g.count(TsTenantId(20)), 1);
    assert_eq!(g.count(TsTenantId(99)), 0); // unseen tenant
    let mut tenants: Vec<u64> = g.tenants().map(|t| t.0).collect();
    tenants.sort_unstable();
    assert_eq!(tenants, vec![10, 20]);
    assert_eq!(g.tenant_count(), 2);
}

#[test]
fn tenanted_allow_and_release() {
    let mut g = TsTenantedGuard::new(1, TsOverflowPolicy::Allow);
    let t = TsTenantId(7);
    g.admit(t).unwrap();
    g.admit(t).unwrap(); // allow climbs past the per-tenant cap
    assert_eq!(g.count(t), 2);
    assert_eq!(g.remaining(t), 0);
    g.release(t);
    assert_eq!(g.count(t), 1);
    assert_eq!(g.max_per_tenant(), 1);
}

// ---------- TsDedupFilter ----------

#[test]
fn dedup_new_then_replay() {
    let mut f = TsDedupFilter::new();
    let k = TsIngestKey::new(1, 100);
    assert!(f.is_new(k)); // first sight
    assert!(!f.is_new(k)); // replay
    assert!(!f.is_new(k));
    assert_eq!(f.seen_count(), 1);
    assert!(f.contains(k));
}

#[test]
fn dedup_distinct_keys_are_independent() {
    let mut f = TsDedupFilter::new();
    assert!(f.is_new(TsIngestKey::new(1, 1)));
    assert!(f.is_new(TsIngestKey::new(1, 2))); // same series, next seq
    assert!(f.is_new(TsIngestKey::new(2, 1))); // diff series, same seq
    assert!(!f.is_new(TsIngestKey::new(1, 1))); // replay of the first
    assert_eq!(f.seen_count(), 3);
}

#[test]
fn dedup_reset_clears() {
    let mut f = TsDedupFilter::with_capacity(8);
    assert!(f.is_empty());
    f.is_new(TsIngestKey::new(5, 5));
    assert!(!f.is_empty());
    f.reset();
    assert_eq!(f.seen_count(), 0);
    assert!(f.is_empty());
    // a key seen before the reset is new again.
    assert!(f.is_new(TsIngestKey::new(5, 5)));
}

// ---------- TsGuardedCollection ----------

fn meta(id: u64, name: &str) -> TsSeriesMetadata {
    TsSeriesMetadata::new(id, name)
}

#[test]
fn guarded_register_up_to_cap_returns_ids() {
    let mut c = TsGuardedCollection::<f64>::new(2, TsOverflowPolicy::Reject);
    assert_eq!(c.register(meta(1, "a")).unwrap(), 1);
    assert_eq!(c.register(meta(2, "b")).unwrap(), 2);
    assert_eq!(c.len(), 2);
    assert_eq!(c.remaining(), 0);
}

#[test]
fn guarded_register_past_cap_errors() {
    let mut c = TsGuardedCollection::<f64>::new(1, TsOverflowPolicy::Reject);
    c.register(meta(1, "a")).unwrap();
    assert_eq!(
        c.register(meta(2, "b")),
        Err(TsCardinalityError::CardinalityCap { max: 1 })
    );
    // the rejected register left no series and no consumed slot.
    assert_eq!(c.len(), 1);
    assert_eq!(c.count(), 1);
}

#[test]
fn guarded_reads_delegate_and_data_intact() {
    let mut c = TsGuardedCollection::<f64>::new(8, TsOverflowPolicy::Reject);
    let id = c.register(meta(42, "cpu")).unwrap();
    assert!(c.push(id, 1, 10.0));
    assert!(c.push(id, 2, 20.0));
    // get by id delegates.
    let s = c.get(id).unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s.last().unwrap().value, 20.0);
    // by_name delegates.
    assert!(c.by_name("cpu").is_some());
    assert!(c.by_name("missing").is_none());
}

#[test]
fn guarded_deregister_frees_a_slot() {
    let mut c = TsGuardedCollection::<f64>::new(1, TsOverflowPolicy::Reject);
    let id = c.register(meta(1, "a")).unwrap();
    assert!(c.register(meta(2, "b")).is_err());
    let removed = c.deregister(id);
    assert!(removed.is_some());
    assert!(c.is_empty());
    // slot reopened.
    assert_eq!(c.register(meta(2, "b")).unwrap(), 2);
}

#[test]
fn guarded_duplicate_register_releases_slot() {
    let mut c = TsGuardedCollection::<f64>::new(4, TsOverflowPolicy::Reject);
    c.register(meta(1, "dup")).unwrap();
    // same id collides inside TsCollection; the guard slot must be returned.
    assert!(c.register(meta(1, "dup")).is_err());
    assert_eq!(c.count(), 1);
    assert_eq!(c.remaining(), 3);
}

#[test]
fn guarded_collection_accessor_exposes_inner() {
    let mut c = TsGuardedCollection::<f64>::new(4, TsOverflowPolicy::Allow);
    c.register(meta(1, "a")).unwrap();
    c.register(meta(2, "b")).unwrap();
    assert_eq!(c.collection().len(), 2);
}
