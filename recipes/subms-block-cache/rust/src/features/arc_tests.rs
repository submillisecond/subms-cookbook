use super::*;

#[test]
fn put_then_get_returns_value() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.get(&1).copied(), Some(10));
    assert_eq!(c.get(&2).copied(), Some(20));
    assert_eq!(c.get(&99), None);
}

#[test]
fn second_hit_promotes_into_t2() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
    c.put(1, 10);
    assert_eq!(c.t1_len(), 1);
    assert_eq!(c.t2_len(), 0);
    // Get promotes into T2.
    c.get(&1);
    assert_eq!(c.t1_len(), 0);
    assert_eq!(c.t2_len(), 1);
}

#[test]
fn capacity_one_evicts_on_every_new_key() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(1);
    assert!(c.put(1, 1).is_none());
    let ev = c.put(2, 2);
    assert!(ev.is_some());
    assert_eq!(c.len(), 1);
    assert!(c.get(&1).is_none());
    assert_eq!(c.get(&2).copied(), Some(2));
}

#[test]
fn scan_resistance_preserves_t2() {
    // Build a frequent set in T2, then run a scan of non-overlapping keys.
    // The scan should pollute T1/B1 but leave T2 alone.
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(8);
    for k in 0u32..4 {
        c.put(k, k);
        c.get(&k); // promote into T2
    }
    assert_eq!(c.t2_len(), 4);
    // Scan: 100 fresh keys. Each goes into T1 and is then evicted to B1.
    for k in 1000u32..1100 {
        c.put(k, k);
    }
    // T2 frequent entries should still be there.
    for k in 0u32..4 {
        assert!(c.get(&k).is_some(), "frequent key {k} was evicted by scan");
    }
}

#[test]
fn ghost_hit_adapts_p() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
    for k in 0u32..8 {
        c.put(k, k);
    }
    // Some early keys are now in B1. Touch one of them; this is a B1
    // hit that should bump `p`.
    let p_before = c.p();
    c.put(0, 100);
    assert!(c.p() >= p_before);
}

#[test]
fn update_in_place_does_not_evict() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(2);
    c.put(1, 10);
    c.put(2, 20);
    let ev = c.put(1, 11);
    assert!(ev.is_none(), "update of existing T1 key should not evict");
    assert_eq!(c.get(&1).copied(), Some(11));
}

#[test]
fn many_inserts_keeps_resident_at_or_below_c() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(16);
    for k in 0u32..1000 {
        c.put(k, k);
        assert!(c.len() <= 16, "resident set exceeded c at k={k}");
    }
}
