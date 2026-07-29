use super::*;
use std::thread;

#[test]
fn empty_snapshot() {
    let t: Art<u32> = Art::new();
    let snap = ArtSnapshot::from_tree(&t);
    assert!(snap.is_empty());
    assert_eq!(snap.len(), 0);
    assert!(snap.get(b"anything").is_none());
}

#[test]
fn snapshot_matches_tree_at_freeze_time() {
    let mut t: Art<u32> = Art::new();
    t.insert(b"alpha", 1);
    t.insert(b"beta", 2);
    let snap = ArtSnapshot::from_tree(&t);
    assert_eq!(snap.len(), 2);
    assert_eq!(snap.get(b"alpha").copied(), Some(1));
    assert_eq!(snap.get(b"beta").copied(), Some(2));
}

#[test]
fn snapshot_isolated_from_writer_mutations() {
    let mut t: Art<u32> = Art::new();
    t.insert(b"alpha", 1);
    t.insert(b"beta", 2);
    let snap = ArtSnapshot::from_tree(&t);

    // Writer mutates after snapshot.
    t.insert(b"gamma", 3);
    t.insert(b"alpha", 99);

    assert_eq!(snap.get(b"alpha").copied(), Some(1), "snapshot frozen");
    assert_eq!(snap.get(b"beta").copied(), Some(2));
    assert!(
        snap.get(b"gamma").is_none(),
        "post-snapshot insert invisible"
    );
    assert_eq!(snap.len(), 2);
}

#[test]
fn snapshot_iteration_is_in_byte_order() {
    let mut t: Art<u32> = Art::new();
    for (i, key) in ["banana", "apple", "cherry", "avocado"].iter().enumerate() {
        t.insert(key.as_bytes(), i as u32);
    }
    let snap = ArtSnapshot::from_tree(&t);
    let keys: Vec<&[u8]> = snap.iter().map(|(k, _)| k).collect();
    assert_eq!(
        keys,
        vec![
            b"apple".as_ref(),
            b"avocado".as_ref(),
            b"banana".as_ref(),
            b"cherry".as_ref(),
        ]
    );
}

#[test]
fn snapshot_sharable_across_threads() {
    let mut t: Art<u32> = Art::new();
    for i in 0..100u32 {
        let k = format!("k{i:03}");
        t.insert(k.as_bytes(), i);
    }
    let snap = ArtSnapshot::from_tree(&t);
    let handles: Vec<_> = (0..4)
        .map(|_| {
            let s = snap.clone();
            thread::spawn(move || {
                let mut hits = 0;
                for i in 0..100 {
                    let k = format!("k{i:03}");
                    if s.get(k.as_bytes()).copied() == Some(i as u32) {
                        hits += 1;
                    }
                }
                hits
            })
        })
        .collect();
    for h in handles {
        assert_eq!(h.join().unwrap(), 100);
    }
}

#[test]
fn snapshot_clone_is_cheap_shared_arc() {
    let mut t: Art<u32> = Art::new();
    for i in 0..10u32 {
        t.insert(&[i as u8], i);
    }
    let a = ArtSnapshot::from_tree(&t);
    let b = a.clone();
    // Same backing storage - confirm by Arc pointer identity.
    assert!(Arc::ptr_eq(&a.entries, &b.entries));
}
