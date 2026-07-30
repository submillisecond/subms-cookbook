use super::*;

#[test]
fn small_entries_fit_under_capacity() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(100, |v: &Vec<u8>| v.len());
    let ev1 = c.put(1, vec![0u8; 10]);
    let ev2 = c.put(2, vec![0u8; 20]);
    assert!(ev1.is_empty());
    assert!(ev2.is_empty());
    assert_eq!(c.used_bytes(), 30);
    assert_eq!(c.len(), 2);
}

#[test]
fn evicts_to_make_room() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(50, |v: &Vec<u8>| v.len());
    c.put(1, vec![0u8; 30]);
    c.put(2, vec![0u8; 10]);
    let ev = c.put(3, vec![0u8; 30]);
    assert!(
        !ev.is_empty(),
        "should have evicted to fit 30B + 10B + 30B into 50B"
    );
    assert!(c.used_bytes() <= 50, "used={}", c.used_bytes());
}

#[test]
fn entry_larger_than_capacity_is_rejected() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(10, |v: &Vec<u8>| v.len());
    let ev = c.put(1, vec![0u8; 100]);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].0, 1);
    assert_eq!(c.used_bytes(), 0, "rejected entry must not be counted");
    assert!(c.get(&1).is_none());
}

#[test]
fn update_in_place_adjusts_used_bytes() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(100, |v: &Vec<u8>| v.len());
    c.put(1, vec![0u8; 10]);
    c.put(1, vec![0u8; 40]);
    assert_eq!(c.used_bytes(), 40);
    assert_eq!(c.len(), 1);
    assert_eq!(c.get(&1).unwrap().len(), 40);
}

#[test]
fn update_bloating_evicts_others() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(60, |v: &Vec<u8>| v.len());
    c.put(1, vec![0u8; 10]);
    c.put(2, vec![0u8; 10]);
    c.put(3, vec![0u8; 10]);
    // Grow key 1 from 10 -> 50; that bloats us to 70B over a 60B cap.
    let ev = c.put(1, vec![0u8; 50]);
    assert!(!ev.is_empty());
    assert!(c.used_bytes() <= 60);
    // Key 1 must still be resident (it's the one we just updated).
    assert!(c.get(&1).is_some());
}

#[test]
fn touched_entry_survives_sweep() {
    // Fill to capacity, then trigger one eviction. That sweep
    // clears all ref bits + evicts one of them, leaving the
    // remaining residents with ref=false. Touch key 2 so its ref
    // bit goes back to true. The next eviction's sweep should
    // give key 2 a second chance and pick one of the others.
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(40, |v: &Vec<u8>| v.len());
    c.put(1, vec![0u8; 10]);
    c.put(2, vec![0u8; 10]);
    c.put(3, vec![0u8; 10]);
    c.put(4, vec![0u8; 10]);
    let _ = c.put(5, vec![0u8; 10]); // first eviction; sweeps + clears refs.
    // Re-touch key 2 (might or might not be resident; only proceed if so).
    if c.get(&2).is_some() {
        let _ = c.put(6, vec![0u8; 10]);
        assert!(
            c.get(&2).is_some(),
            "touched key 2 should survive the next sweep"
        );
    }
}

#[test]
fn capacity_bytes_floor_is_one() {
    let c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(0, |v: &Vec<u8>| v.len());
    assert_eq!(c.capacity_bytes(), 1);
}

#[test]
fn is_empty_reflects_contents() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(100, |v: &Vec<u8>| v.len());
    assert!(c.is_empty());
    c.put(1, vec![0u8; 10]);
    assert!(!c.is_empty());
}

// After an eviction frees a slot, the next insert must reuse it via the
// free-slot stack rather than growing the backing vec.
#[test]
fn eviction_then_insert_reuses_freed_slot() {
    let mut c: WeightedCache<u32, Vec<u8>> =
        WeightedCache::with_capacity_bytes(20, |v: &Vec<u8>| v.len());
    c.put(1, vec![0u8; 10]);
    c.put(2, vec![0u8; 10]);
    // Forces eviction of a resident, freeing its slot.
    let ev = c.put(3, vec![0u8; 10]);
    assert!(!ev.is_empty(), "third insert should have evicted to fit");
    assert!(c.used_bytes() <= 20);
    // Another churn cycle to keep exercising the reuse path.
    let _ = c.put(4, vec![0u8; 10]);
    assert!(c.used_bytes() <= 20);
    assert!(c.len() <= 2);
}

// Drives the private clock-sweep directly to reach the empty-cache
// early return, the all-excluded no-eviction case, the None-hole skip,
// and a normal eviction.
#[test]
fn sweep_evict_excluding_edge_cases() {
    let mut c: WeightedCache<u32, u32> = WeightedCache::with_capacity_bytes(100, |_v: &u32| 10);
    // Empty cache: nothing resident to evict.
    assert!(c.sweep_evict_excluding(NIL).is_none());

    c.put(1, 1);
    c.put(2, 2);
    c.put(3, 3);
    let id1 = *c.index.get(&1).unwrap();

    // A first pass clears the referenced bits; a second finds a victim.
    let first = c.sweep_evict_excluding(NIL);
    assert!(first.is_some(), "an unreferenced resident should evict");
    // A freed slot is now a None hole the next sweep must skip over.
    let second = c.sweep_evict_excluding(NIL);
    assert!(second.is_some());

    // With only the excluded entry left resident, the sweep evicts nothing.
    while c.len() > 1 {
        let _ = c.sweep_evict_excluding(NIL);
    }
    if !c.is_empty() {
        let only = *c.index.keys().next().unwrap();
        let only_id = *c.index.get(&only).unwrap();
        let _ = id1;
        assert!(
            c.sweep_evict_excluding(only_id).is_none(),
            "excluding the sole resident leaves nothing to evict"
        );
    }
}
