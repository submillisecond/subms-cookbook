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

#[test]
fn accessors_report_list_sizes() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
    assert_eq!(c.capacity(), 4);
    assert!(c.is_empty());
    c.put(1, 10);
    assert!(!c.is_empty());
    assert_eq!(c.t1_len(), 1);
    assert_eq!(c.t2_len(), 0);
    assert_eq!(c.b1_len(), 0);
    assert_eq!(c.b2_len(), 0);
    assert_eq!(c.p(), 0);
    c.get(&1);
    assert_eq!(c.t1_len(), 0);
    assert_eq!(c.t2_len(), 1);
}

// Deterministic walk that lands entries on every one of the four lists
// and exercises the B1-hit / B2-hit adaptation cases plus a T2->B2
// eviction. Traced by hand against the ARC replacement policy.
#[test]
fn ghost_lists_and_adaptation_paths() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(2);
    c.put(1, 10); // T1=[1]
    c.get(&1); // promote -> T2=[1], T1=[]
    c.put(2, 20); // T1=[2]
    c.put(3, 30); // Case IV: replace evicts T1 tail (2) -> B1; T1=[3]
    assert_eq!(c.b1_len(), 1, "key 2 should have been ghosted to B1");
    // A ghost key is present in the index but returns None on get.
    assert!(c.get(&2).is_none(), "B1 ghost must not read as a resident hit");

    // B1 hit: re-put key 2 (currently in B1). Grows p, replaces (which
    // evicts the T2 LRU key 1 into B2), and moves 2 into T2.
    let evicted = c.put(2, 22);
    assert_eq!(evicted, Some((1, 10)), "the B1-hit replace should evict T2 LRU");
    assert_eq!(c.b2_len(), 1, "key 1 should now sit in B2");
    assert_eq!(c.get(&2).copied(), Some(22));

    // B2 hit: re-put key 1 (currently in B2). Shrinks p, replaces, moves
    // 1 into T2.
    let _ = c.put(1, 111);
    assert_eq!(c.get(&1).copied(), Some(111));
}

// A T2 entry updated in place stays in T2 and evicts nothing.
#[test]
fn update_of_t2_entry_is_in_place() {
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(4);
    c.put(7, 70);
    c.get(&7); // -> T2
    assert_eq!(c.t2_len(), 1);
    let ev = c.put(7, 71);
    assert!(ev.is_none(), "in-place T2 update must not evict");
    assert_eq!(c.t2_len(), 1);
    assert_eq!(c.get(&7).copied(), Some(71));
}

// Heavy mixed workload with strong reuse. Drives keys through all four
// lists, forces the fully-saturated |L1|+|L2| == 2c branch (which pops
// the B2 LRU), and exercises the doubly-linked-list unlink/push paths
// for interior nodes. Asserts the ARC resident invariant throughout.
#[test]
fn saturating_stress_exercises_every_list() {
    let c_cap = 4usize;
    let mut c: ArcCache<u32, u32> = ArcCache::with_capacity(c_cap);
    let working = 12u32;
    let mut saw_b1 = false;
    let mut saw_b2 = false;
    let mut saw_full_l2 = false;
    for i in 0u32..8000 {
        let key = i % working;
        c.put(key, i);
        if i % 2 == 0 {
            let _ = c.get(&((i / 3) % working));
        }
        if i % 3 == 0 {
            // Re-touch a likely-ghost key to provoke B1/B2 hits.
            let _ = c.put((i / 5) % working, i);
        }
        assert!(c.len() <= c_cap, "resident set exceeded c at i={i}");
        if c.b1_len() > 0 {
            saw_b1 = true;
        }
        if c.b2_len() > 0 {
            saw_b2 = true;
        }
        if c.t2_len() + c.b2_len() >= c_cap {
            saw_full_l2 = true;
        }
    }
    assert!(saw_b1, "workload never populated B1");
    assert!(saw_b2, "workload never populated B2");
    assert!(saw_full_l2, "workload never saturated L2");
}
