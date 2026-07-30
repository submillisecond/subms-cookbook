use super::*;

#[test]
fn empty_persistent_state() {
    let t: PersistentTreap<i32, i32> = PersistentTreap::new(0);
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
    assert!(t.get(&1).is_none());
}

#[test]
fn insert_returns_new_version_without_touching_old() {
    let v0: PersistentTreap<i32, &'static str> = PersistentTreap::new(7);
    let v1 = v0.insert(1, "one");
    assert_eq!(v0.len(), 0);
    assert_eq!(v1.len(), 1);
    assert!(v0.get(&1).is_none());
    assert_eq!(v1.get(&1).copied(), Some("one"));
}

#[test]
fn version_chain_each_isolated() {
    let v0: PersistentTreap<i32, i32> = PersistentTreap::new(7);
    let v1 = v0.insert(1, 10);
    let v2 = v1.insert(2, 20);
    let v3 = v2.insert(3, 30);

    assert_eq!(v0.len(), 0);
    assert_eq!(v1.len(), 1);
    assert_eq!(v2.len(), 2);
    assert_eq!(v3.len(), 3);

    assert!(v1.get(&2).is_none());
    assert_eq!(v2.get(&2).copied(), Some(20));
    assert_eq!(v3.get(&3).copied(), Some(30));
    assert!(v1.get(&3).is_none());
}

#[test]
fn remove_leaves_old_version_intact() {
    let v0: PersistentTreap<i32, &'static str> = PersistentTreap::new(7);
    let v1 = v0.insert(1, "one").insert(2, "two").insert(3, "three");
    let v2 = v1.remove(&2);
    assert_eq!(
        v1.get(&2).copied(),
        Some("two"),
        "v1 still has the removed key"
    );
    assert!(v2.get(&2).is_none());
    assert_eq!(v1.len(), 3);
    assert_eq!(v2.len(), 2);
}

#[test]
fn insert_replaces_value_in_new_version_only() {
    let v0: PersistentTreap<i32, &'static str> = PersistentTreap::new(7);
    let v1 = v0.insert(1, "first");
    let v2 = v1.insert(1, "second");
    assert_eq!(v1.get(&1).copied(), Some("first"));
    assert_eq!(v2.get(&1).copied(), Some("second"));
    assert_eq!(v1.len(), 1);
    assert_eq!(v2.len(), 1);
}

#[test]
fn in_order_yields_sorted_keys() {
    let mut t: PersistentTreap<i32, i32> = PersistentTreap::new(123);
    for k in [5, 1, 9, 3, 7, 2, 8] {
        t = t.insert(k, k * 10);
    }
    let keys: Vec<i32> = t.collect_in_order().into_iter().map(|(k, _)| k).collect();
    assert_eq!(keys, vec![1, 2, 3, 5, 7, 8, 9]);
}

#[test]
fn remove_absent_key_returns_clone() {
    let v0: PersistentTreap<i32, i32> = PersistentTreap::new(0);
    let v1 = v0.insert(1, 1).insert(2, 2);
    let v2 = v1.remove(&999);
    assert_eq!(v1.len(), v2.len());
    assert_eq!(v2.get(&1).copied(), Some(1));
    assert_eq!(v2.get(&2).copied(), Some(2));
}

#[test]
fn clone_shares_structure_and_stays_isolated() {
    let v1: PersistentTreap<i32, i32> = PersistentTreap::new(3).insert(1, 10).insert(2, 20);
    let c = v1.clone();
    assert_eq!(c.len(), v1.len());
    assert_eq!(c.get(&1).copied(), Some(10));
    assert_eq!(c.get(&2).copied(), Some(20));
    // A mutation off the clone leaves the clone (and the original) untouched.
    let c2 = c.insert(3, 30);
    assert!(c.get(&3).is_none());
    assert_eq!(c2.get(&3).copied(), Some(30));
}

#[test]
fn remove_every_key_exercises_merges() {
    let mut t: PersistentTreap<i32, i32> = PersistentTreap::new(7);
    let keys = [8, 3, 12, 1, 5, 10, 14, 2, 4, 6, 9, 11, 13, 7, 0];
    for &k in &keys {
        t = t.insert(k, k * 100);
    }
    // Remove interior nodes (two children) in a mixed order to drive both
    // merge_subtrees priority branches and its recursion.
    let mut remaining: Vec<i32> = keys.to_vec();
    for &k in &[8, 5, 12, 3, 10] {
        t = t.remove(&k);
        remaining.retain(|&x| x != k);
        assert!(t.get(&k).is_none());
        for &r in &remaining {
            assert_eq!(t.get(&r).copied(), Some(r * 100), "key {r} survives");
        }
    }
    let sorted: Vec<i32> = t.collect_in_order().into_iter().map(|(k, _)| k).collect();
    let mut expect = remaining.clone();
    expect.sort_unstable();
    assert_eq!(sorted, expect);
}

#[test]
fn many_versions_stress() {
    let mut versions: Vec<PersistentTreap<i32, i32>> = vec![PersistentTreap::new(99)];
    for i in 0..200 {
        let next = versions.last().unwrap().insert(i, i * 2);
        versions.push(next);
    }
    // Every version v_i has exactly i entries 0..i.
    for (i, v) in versions.iter().enumerate() {
        assert_eq!(v.len(), i);
        for k in 0..i as i32 {
            assert_eq!(v.get(&k).copied(), Some(k * 2));
        }
    }
}
