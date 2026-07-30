use super::*;
use std::sync::Arc as StdArc;
use std::thread;

fn build_treap(keys: &[i32]) -> Treap<i32, i32> {
    let mut t: Treap<i32, i32> = Treap::new(42);
    for &k in keys {
        t.insert(k, k * 10);
    }
    t
}

#[test]
fn empty_snapshot_state() {
    let t: Treap<i32, i32> = Treap::new(0);
    let snap = TreapSnapshot::from_treap(&t);
    assert!(snap.is_empty());
    assert_eq!(snap.len(), 0);
    assert!(snap.get(&1).is_none());
}

#[test]
fn snapshot_get_returns_treap_value() {
    let t = build_treap(&[3, 1, 4, 1, 5, 9, 2, 6]);
    let snap = TreapSnapshot::from_treap(&t);
    for &k in &[1, 2, 3, 4, 5, 6, 9] {
        assert_eq!(snap.get(&k).copied(), Some(k * 10));
    }
    assert!(snap.get(&999).is_none());
}

#[test]
fn snapshot_isolated_from_subsequent_writes() {
    let mut t = build_treap(&[1, 2, 3]);
    let snap = TreapSnapshot::from_treap(&t);
    t.insert(4, 40);
    t.remove(&1);
    // Snapshot was taken pre-mutation - it must NOT see the new key
    // and MUST still see the removed one.
    assert!(snap.get(&4).is_none());
    assert_eq!(snap.get(&1).copied(), Some(10));
    assert_eq!(snap.len(), 3);
}

#[test]
fn clone_is_cheap_pointer_bump() {
    let t = build_treap(&[1, 2, 3]);
    let snap = TreapSnapshot::from_treap(&t);
    let snap2 = snap.clone();
    // Both Arcs point at the same inner.
    assert!(StdArc::ptr_eq(&snap.inner, &snap2.inner));
}

#[test]
fn iter_is_sorted() {
    let t = build_treap(&[5, 1, 9, 3, 7, 2, 8, 4, 6]);
    let snap = TreapSnapshot::from_treap(&t);
    let keys: Vec<i32> = snap.iter().map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![1, 2, 3, 4, 5, 6, 7, 8, 9]);
}

#[test]
fn range_yields_sorted_window() {
    let t = build_treap(&[1, 2, 3, 4, 5, 6, 7, 8, 9]);
    let snap = TreapSnapshot::from_treap(&t);
    let keys: Vec<i32> = snap.range(&3, &7).map(|(k, _)| *k).collect();
    assert_eq!(keys, vec![3, 4, 5, 6, 7]);
}

#[test]
fn readers_under_writer_load() {
    // Take a snapshot, hand clones to N reader threads, mutate the
    // source treap on the main thread. Every reader must see the
    // exact snapshot state independent of the writer's actions.
    let mut t = build_treap(&(0..200i32).collect::<Vec<_>>());
    let snap = TreapSnapshot::from_treap(&t);

    let handles: Vec<_> = (0..4)
        .map(|_| {
            let s = snap.clone();
            thread::spawn(move || {
                let mut ok = true;
                for k in 0..200 {
                    if s.get(&k).copied() != Some(k * 10) {
                        ok = false;
                        break;
                    }
                }
                ok
            })
        })
        .collect();

    // Concurrent writer churn on the source - snapshot is unaffected.
    for k in 200..400 {
        t.insert(k, k * 10);
    }
    for k in 0..100 {
        t.remove(&k);
    }

    for h in handles {
        assert!(h.join().unwrap(), "reader observed mutation");
    }
    assert_eq!(snap.len(), 200);
}

#[test]
fn snapshot_outlives_treap() {
    let snap;
    {
        let t = build_treap(&[1, 2, 3]);
        snap = TreapSnapshot::from_treap(&t);
        // `t` drops at the end of this block.
    }
    assert_eq!(snap.get(&2).copied(), Some(20));
    assert_eq!(snap.len(), 3);
}
