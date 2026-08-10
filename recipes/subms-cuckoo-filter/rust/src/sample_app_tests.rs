//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the open-order set deletes filled orders, and each feature holds the
//! property its section leans on.

use super::*;

/// The gateway section: NEW inserts, FILL and CANCEL delete, and the closed
/// ids stop answering yes.
#[test]
fn oms_gateway_scenario() {
    let mut open = CuckooFilter::with_capacity(10_000);
    for oid in ["ORD-1001", "ORD-1002", "ORD-1003", "ORD-1004"] {
        open.insert(oid);
    }
    assert_eq!(open.len(), 4);

    assert!(open.contains("ORD-1002"));
    assert!(open.delete("ORD-1002"), "a fill closes the order out");
    open.insert("ORD-1005");
    assert!(open.delete("ORD-1003"), "a cancel closes the order out");
    assert!(open.delete("ORD-1005"));
    assert!(
        !open.insert_if_absent("ORD-1004"),
        "a replayed NEW must not add a second fingerprint"
    );

    assert!(!open.contains("ORD-1002"));
    assert!(!open.contains("ORD-1003"));
    for oid in ["ORD-1001", "ORD-1004"] {
        assert!(
            open.contains(oid),
            "a stored order must always report present"
        );
    }
    assert_eq!(open.len(), 2);
}

/// The checkpoint section: the live set survives a write/parse round trip.
#[test]
fn checkpoint_restores_the_live_set() {
    let mut open = CuckooFilter::with_capacity(10_000);
    for oid in ["ORD-1001", "ORD-1004"] {
        open.insert(oid);
    }
    let mut buf = Vec::new();
    open.write_to(&mut buf).unwrap();
    let restored = CuckooFilter::parse(&buf).unwrap();
    assert_eq!(restored.len(), 2);
    assert!(restored.contains("ORD-1001"));
    assert!(restored.contains("ORD-1004"));
}

/// The fan-in section: same geometry merges, a different one is refused.
#[test]
fn shard_fan_in_merges_and_refuses_a_mismatch() {
    let mut shard_a = CuckooFilter::with_capacity(10_000);
    let mut shard_b = CuckooFilter::with_capacity(10_000);
    for i in 0..500u32 {
        shard_a.insert(&format!("A-ORD-{i}"));
        shard_b.insert(&format!("B-ORD-{i}"));
    }
    shard_a.union(&shard_b).unwrap();
    assert_eq!(shard_a.len(), 1_000);
    assert!(shard_a.contains("A-ORD-7"));
    assert!(shard_a.contains("B-ORD-7"));

    let mismatched = CuckooFilter::with_capacity(1_000_000);
    assert!(shard_a.union(&mismatched).is_err());
}

/// The session-roll section: `clear` empties the set and keeps the array.
#[test]
fn session_roll_empties_the_set() {
    let mut open = CuckooFilter::with_capacity(10_000);
    open.insert("ORD-1001");
    let bytes = open.size_in_bytes();
    open.clear();
    assert!(open.is_empty());
    assert!(!open.contains("ORD-1001"));
    assert_eq!(open.size_in_bytes(), bytes, "the allocation is reused");
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
