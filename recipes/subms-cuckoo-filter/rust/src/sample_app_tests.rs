//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the open-order set deletes filled orders, and each feature holds the
//! property its section leans on.

use super::*;

#[test]
fn open_order_set_scenario() {
    let mut open = CuckooFilter::with_capacity(10_000);
    for oid in ["ORD-1001", "ORD-1002", "ORD-1003", "ORD-1004"] {
        open.insert(oid);
    }
    assert_eq!(open.len(), 4);

    assert!(open.contains("ORD-1002"));
    assert!(open.delete("ORD-1002"), "a live order can be deleted");
    assert!(
        !open.contains("ORD-1002"),
        "a filled order leaves the live set"
    );

    for oid in ["ORD-1001", "ORD-1003", "ORD-1004"] {
        assert!(
            open.contains(oid),
            "a stored order must always report present"
        );
    }
    assert_eq!(open.len(), 3);
}

#[cfg(feature = "variable-fingerprint")]
#[test]
fn variable_fingerprint_lowers_false_positives() {
    use crate::{FingerprintWidth, VariableFpCuckooFilter};
    let n = 5_000usize;
    let mut narrow = VariableFpCuckooFilter::new(n, FingerprintWidth::Eight);
    let mut wide = VariableFpCuckooFilter::new(n, FingerprintWidth::Sixteen);
    for i in 0..n {
        narrow.insert(&format!("RESTRICTED-{i}"));
        wide.insert(&format!("RESTRICTED-{i}"));
    }
    let (mut narrow_fp, mut wide_fp) = (0usize, 0usize);
    for i in 0..10_000usize {
        let sym = format!("TRADABLE-{i}");
        if narrow.contains(&sym) {
            narrow_fp += 1;
        }
        if wide.contains(&sym) {
            wide_fp += 1;
        }
    }
    assert!(
        wide_fp < narrow_fp,
        "wide_fp={wide_fp} should be < narrow_fp={narrow_fp}"
    );
}

#[cfg(feature = "dynamic")]
#[test]
fn dynamic_grows_and_keeps_every_id() {
    use crate::DynamicCuckooFilter;
    let mut seen = DynamicCuckooFilter::with_threshold(1_000, 0.5);
    for i in 0..20_000u32 {
        seen.insert(&format!("MSG-{i}"));
    }
    assert!(seen.layer_count() > 1, "grew past the initial layer");
    for i in 0..20_000u32 {
        assert!(
            seen.contains(&format!("MSG-{i}")),
            "no id dropped as the window grew"
        );
    }
}

#[cfg(feature = "concurrent-reads")]
#[test]
fn snapshot_is_frozen_against_later_writes() {
    use crate::CuckooSnapshot;
    let mut open = CuckooFilter::with_capacity(10_000);
    for i in 0..1_000u32 {
        open.insert(&format!("ORD-{i}"));
    }
    let snap = CuckooSnapshot::capture(&open);

    open.insert("ORD-LATE");
    open.delete("ORD-0");

    let matched = (0..1_000u32)
        .filter(|i| snap.contains(&format!("ORD-{i}")))
        .count();
    assert_eq!(matched, 1_000, "the snapshot keeps the whole captured set");
    assert!(
        snap.contains("ORD-0"),
        "snapshot retains a key the writer later deleted"
    );
    assert!(
        !snap.contains("ORD-LATE"),
        "snapshot never sees the writer's later insert"
    );
}

#[cfg(feature = "compressed-buckets")]
#[test]
fn compressed_footprint_beats_base_at_moderate_load() {
    use crate::CompressedCuckooFilter;
    let mut cf = CompressedCuckooFilter::with_capacity(10_000);
    for i in 0..3_000u32 {
        cf.insert(&format!("ORD-{i}"));
    }
    let base_fixed_bytes = cf.bucket_count() * 4;
    assert!(
        cf.occupied_bytes() < base_fixed_bytes,
        "sorted-run encoding is smaller here"
    );
    for i in 0..3_000u32 {
        assert!(cf.contains(&format!("ORD-{i}")));
    }
    assert!(cf.delete("ORD-0"));
}
