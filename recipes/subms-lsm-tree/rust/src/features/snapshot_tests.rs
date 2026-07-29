use super::*;

#[test]
fn snapshot_sees_current_manifest_at_take_time() {
    let mgr = SnapshotManager::new();
    mgr.publish(SnapshotManifest::new(vec![1, 2, 3]));
    let snap = mgr.snapshot();
    assert_eq!(snap.sstable_ids(), &[1, 2, 3]);
}

#[test]
fn snapshot_is_isolated_from_subsequent_publish() {
    let mgr = SnapshotManager::new();
    mgr.publish(SnapshotManifest::new(vec![1]));
    let snap = mgr.snapshot();
    mgr.publish(SnapshotManifest::new(vec![1, 2, 3, 4]));
    assert_eq!(
        snap.sstable_ids(),
        &[1],
        "snapshot must not see post-snapshot publishes"
    );
    assert_eq!(mgr.current_ids(), vec![1, 2, 3, 4]);
}

#[test]
fn multiple_concurrent_snapshots_are_independent() {
    let mgr = SnapshotManager::new();
    mgr.publish(SnapshotManifest::new(vec![10]));
    let s1 = mgr.snapshot();
    mgr.publish(SnapshotManifest::new(vec![10, 20]));
    let s2 = mgr.snapshot();
    mgr.publish(SnapshotManifest::new(vec![10, 20, 30]));
    let s3 = mgr.snapshot();
    assert_eq!(s1.sstable_ids(), &[10]);
    assert_eq!(s2.sstable_ids(), &[10, 20]);
    assert_eq!(s3.sstable_ids(), &[10, 20, 30]);
}

#[test]
fn snapshot_ids_are_monotonic() {
    let mgr = SnapshotManager::new();
    let a = mgr.snapshot();
    let b = mgr.snapshot();
    let c = mgr.snapshot();
    assert!(a.id() < b.id() && b.id() < c.id());
}

#[test]
fn snapshot_clone_is_cheap_and_consistent() {
    let mgr = SnapshotManager::new();
    mgr.publish(SnapshotManifest::new(vec![7, 8, 9]));
    let s = mgr.snapshot();
    let s2 = s.clone();
    // Arc::ptr_eq via underlying manifest.
    assert!(Arc::ptr_eq(&s.manifest, &s2.manifest));
    assert_eq!(s2.sstable_ids(), &[7, 8, 9]);
}

#[test]
fn publish_with_transforms_current() {
    let mgr = SnapshotManager::new();
    mgr.publish(SnapshotManifest::new(vec![1, 2]));
    mgr.publish_with(|cur| {
        let mut ids = cur.sstable_ids.clone();
        ids.push(3);
        SnapshotManifest::new(ids)
    });
    assert_eq!(mgr.current_ids(), vec![1, 2, 3]);
}

#[test]
fn empty_initial_snapshot_is_empty() {
    let mgr = SnapshotManager::new();
    let s = mgr.snapshot();
    assert!(s.manifest().is_empty());
    assert_eq!(s.manifest().len(), 0);
}
