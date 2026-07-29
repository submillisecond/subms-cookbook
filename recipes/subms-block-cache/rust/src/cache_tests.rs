use super::*;

#[test]
fn get_returns_inserted_value() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(4);
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.get(&1).copied(), Some(10));
    assert_eq!(c.get(&2).copied(), Some(20));
    assert_eq!(c.get(&999), None);
}

#[test]
fn update_in_place() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(4);
    c.put(1, 10);
    let evicted = c.put(1, 11);
    assert!(evicted.is_none());
    assert_eq!(c.get(&1).copied(), Some(11));
    assert_eq!(c.len(), 1);
}

#[test]
fn evicts_when_full() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(3);
    c.put(1, 10);
    c.put(2, 20);
    c.put(3, 30);
    // All freshly inserted -> all referenced. Inserting 4 will sweep and
    // clear refs, eventually evicting one.
    let evicted = c.put(4, 40);
    assert!(evicted.is_some(), "should have evicted on insert at full");
    assert_eq!(c.len(), 3);
    assert_eq!(c.get(&4).copied(), Some(40));
}

#[test]
fn touched_key_survives_next_eviction() {
    // After the first sweep clears all referenced bits, touching a key
    // sets its bit again. The next insert at full capacity is then forced
    // to skip that touched key on its sweep.
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(3);
    c.put(1, 10);
    c.put(2, 20);
    c.put(3, 30);
    let _ = c.put(4, 40); // First eviction sweep (evicts key 1).
    let _ = c.get(&4); // Re-touch key 4 so its ref bit is set.
    let _ = c.put(5, 50); // Second sweep; key 4 should survive.
    assert!(c.get(&4).is_some(), "key 4 was just touched, should stay");
}

#[test]
fn capacity_floor_is_one() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(0);
    assert_eq!(c.capacity(), 1);
    c.put(1, 10);
    assert_eq!(c.get(&1).copied(), Some(10));
}

#[test]
fn len_and_is_empty() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(4);
    assert!(c.is_empty());
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.len(), 2);
    assert!(!c.is_empty());
}

#[test]
fn get_on_missing_returns_none() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(4);
    assert!(c.get(&42).is_none());
    c.put(1, 1);
    assert!(c.get(&42).is_none());
}

#[test]
fn evictions_match_insert_count_beyond_capacity() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(3);
    let mut evictions = 0;
    for i in 0..10u32 {
        if c.put(i, i).is_some() {
            evictions += 1;
        }
    }
    // 10 inserts into capacity 3 -> 7 evictions.
    assert_eq!(evictions, 7);
    assert_eq!(c.len(), 3);
}

#[test]
fn capacity_one_evicts_on_every_insert() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(1);
    c.put(1, 10);
    let ev = c.put(2, 20);
    assert!(ev.is_some());
    assert_eq!(ev.unwrap(), (1, 10));
    assert!(c.get(&1).is_none());
    assert_eq!(c.get(&2).copied(), Some(20));
}

#[test]
fn mixed_workload_preserves_invariants() {
    let mut c: BlockCache<u32, u32> = BlockCache::with_capacity(5);
    for i in 0..20u32 {
        c.put(i, i * 10);
        if i % 3 == 0 {
            let _ = c.get(&(i / 2));
        }
    }
    assert_eq!(c.len(), 5);
}
