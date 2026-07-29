use super::*;

#[test]
fn empty_top_is_empty() {
    let hh = HeavyHitters::new(5, 5, 1024);
    assert!(hh.top().is_empty());
}

#[test]
fn fewer_than_k_distinct_keys_all_present() {
    let mut hh = HeavyHitters::new(10, 5, 1024);
    for _ in 0..100 {
        hh.add("a");
    }
    for _ in 0..50 {
        hh.add("b");
    }
    for _ in 0..25 {
        hh.add("c");
    }
    let top = hh.top();
    assert_eq!(top.len(), 3);
    // Descending order by estimate.
    assert_eq!(top[0].key, "a");
    assert_eq!(top[1].key, "b");
    assert_eq!(top[2].key, "c");
    assert!(top[0].estimate >= top[1].estimate);
    assert!(top[1].estimate >= top[2].estimate);
}

#[test]
fn cold_keys_evicted_when_hotter_arrive() {
    let mut hh = HeavyHitters::new(2, 5, 1024);
    for _ in 0..10 {
        hh.add("cold");
    }
    for _ in 0..20 {
        hh.add("warm");
    }
    // Both fit. Now add a hot key that should evict "cold".
    for _ in 0..100 {
        hh.add("hot");
    }
    let top = hh.top();
    assert_eq!(top.len(), 2);
    let keys: Vec<&str> = top.iter().map(|e| e.key.as_str()).collect();
    assert!(keys.contains(&"hot"));
    assert!(keys.contains(&"warm"));
    assert!(!keys.contains(&"cold"));
    // First entry is the hottest.
    assert_eq!(top[0].key, "hot");
}

#[test]
fn existing_top_key_refreshed_in_place() {
    let mut hh = HeavyHitters::new(3, 5, 1024);
    hh.add("a");
    hh.add("b");
    hh.add("c");
    // Bump "c" past "a" and "b".
    for _ in 0..50 {
        hh.add("c");
    }
    let top = hh.top();
    assert_eq!(top.len(), 3);
    assert_eq!(top[0].key, "c");
    assert!(top[0].estimate >= 50);
}

#[test]
fn k_one_tracks_only_hottest() {
    let mut hh = HeavyHitters::new(1, 5, 1024);
    for _ in 0..3 {
        hh.add("low");
    }
    for _ in 0..10 {
        hh.add("high");
    }
    let top = hh.top();
    assert_eq!(top.len(), 1);
    assert_eq!(top[0].key, "high");
}

#[test]
fn ties_do_not_churn_existing_entries() {
    // Equal counts: the first occupant of the slot wins.
    let mut hh = HeavyHitters::new(2, 5, 1024);
    for _ in 0..5 {
        hh.add("first");
    }
    for _ in 0..5 {
        hh.add("second");
    }
    // Now "third" reaches the same count as the floor entry.
    for _ in 0..5 {
        hh.add("third");
    }
    let top = hh.top();
    let keys: Vec<&str> = top.iter().map(|e| e.key.as_str()).collect();
    // "third" arrived after the slots were full at est=5, so it
    // should NOT have evicted either incumbent on the strict-greater rule.
    assert!(keys.contains(&"first"));
    assert!(keys.contains(&"second"));
    assert!(!keys.contains(&"third"));
}

#[test]
fn k_floor_enforced() {
    // k=0 should be clamped to 1 (otherwise the top set is useless).
    let hh = HeavyHitters::new(0, 5, 1024);
    assert_eq!(hh.k(), 1);
}
