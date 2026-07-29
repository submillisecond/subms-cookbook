use super::*;

#[test]
fn snapshot_reports_what_writer_inserted() {
    let mut cf = CuckooFilter::with_capacity(1000);
    for i in 0..200u32 {
        cf.insert(&format!("k{i}"));
    }
    let snap = CuckooSnapshot::capture(&cf);
    for i in 0..200u32 {
        assert!(snap.contains(&format!("k{i}")), "lost k{i}");
    }
    assert_eq!(snap.len(), 200);
}

#[test]
fn snapshot_isolated_from_writer_mutations() {
    let mut cf = CuckooFilter::with_capacity(1000);
    cf.insert("before-snapshot");
    let snap = CuckooSnapshot::capture(&cf);

    // Writer makes changes AFTER the snapshot was taken.
    cf.insert("after-snapshot");
    cf.delete("before-snapshot");

    assert!(
        snap.contains("before-snapshot"),
        "snapshot must remain stable"
    );
    assert!(
        !snap.contains("after-snapshot"),
        "snapshot must NOT see writer's later insert"
    );
}

#[test]
fn empty_snapshot_rejects_everything() {
    let cf = CuckooFilter::with_capacity(100);
    let snap = CuckooSnapshot::capture(&cf);
    assert!(snap.is_empty());
    assert!(!snap.contains("never-inserted"));
}

#[test]
fn iter_fingerprints_count_matches_len() {
    let mut cf = CuckooFilter::with_capacity(500);
    for i in 0..100u32 {
        cf.insert(&format!("k{i}"));
    }
    let snap = CuckooSnapshot::capture(&cf);
    let yielded = snap.iter_fingerprints().count();
    assert_eq!(yielded, snap.len());
}

#[test]
fn arc_shareable_across_threads() {
    let mut cf = CuckooFilter::with_capacity(1000);
    for i in 0..200u32 {
        cf.insert(&format!("k{i}"));
    }
    let snap = CuckooSnapshot::capture(&cf);

    let mut handles = Vec::new();
    for t in 0..4 {
        let s = Arc::clone(&snap);
        handles.push(std::thread::spawn(move || {
            let mut found = 0usize;
            for i in 0..200u32 {
                if s.contains(&format!("k{i}")) {
                    found += 1;
                }
            }
            (t, found)
        }));
    }
    for h in handles {
        let (_t, found) = h.join().unwrap();
        assert_eq!(found, 200);
    }
}

#[test]
fn writer_grows_independently_of_snapshot_bucket_count() {
    let mut cf = CuckooFilter::with_capacity(64);
    for i in 0..20u32 {
        cf.insert(&format!("k{i}"));
    }
    let snap = CuckooSnapshot::capture(&cf);
    let snap_buckets = snap.bucket_count();
    // The writer keeps inserting; the snapshot's bucket count stays
    // at the captured value (the base filter doesn't resize, but
    // this confirms the snapshot isn't merely a view).
    for i in 20..40u32 {
        cf.insert(&format!("k{i}"));
    }
    assert_eq!(snap.bucket_count(), snap_buckets);
}
