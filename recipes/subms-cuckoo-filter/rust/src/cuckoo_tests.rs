use super::*;

#[test]
fn insert_contains_delete_round_trip() {
    let mut cf = CuckooFilter::with_capacity(1000);
    for i in 0..500u32 {
        assert!(cf.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.contains(&format!("k{i}")), "k{i} should be present");
    }
    for i in 0..500u32 {
        assert!(cf.delete(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(!cf.contains(&format!("k{i}")), "k{i} should be gone");
    }
    assert_eq!(cf.len(), 0);
}

#[test]
fn delete_nonexistent_returns_false() {
    let mut cf = CuckooFilter::with_capacity(100);
    assert!(!cf.delete("never-inserted"));
}

#[test]
fn empty_contains_returns_false() {
    let cf = CuckooFilter::with_capacity(100);
    assert!(!cf.contains("anything"));
}

#[test]
fn false_positive_rate_under_three_percent() {
    let n = 10_000;
    let mut cf = CuckooFilter::with_capacity(n);
    for i in 0..n {
        assert!(cf.insert(&format!("present{i}")));
    }
    let probes = 10_000;
    let mut fp = 0usize;
    for i in 0..probes {
        if cf.contains(&format!("absent{i}")) {
            fp += 1;
        }
    }
    let fpr = fp as f64 / probes as f64;
    // Cuckoo with 8-bit fingerprints + bucket 4 lands around 0.3% theoretical;
    // 3% is generous headroom for the FNV+SplitMix hash combo.
    assert!(fpr < 0.03, "fpr {fpr:.4} too high");
}

#[test]
fn bucket_count_is_power_of_two() {
    let cf = CuckooFilter::with_capacity(1000);
    let n = cf.bucket_count();
    assert!(n.is_power_of_two(), "{n} should be power of 2");
}

#[test]
fn len_tracks_inserts_and_deletes() {
    let mut cf = CuckooFilter::with_capacity(1000);
    assert_eq!(cf.len(), 0);
    cf.insert("a");
    cf.insert("b");
    assert_eq!(cf.len(), 2);
    cf.delete("a");
    assert_eq!(cf.len(), 1);
    cf.delete("absent");
    assert_eq!(cf.len(), 1);
}

#[test]
fn is_empty_initially() {
    let cf = CuckooFilter::with_capacity(100);
    assert!(cf.is_empty());
}

#[test]
fn duplicate_insert_increases_count() {
    // Cuckoo allows multiple entries per key (up to bucket capacity).
    let mut cf = CuckooFilter::with_capacity(100);
    cf.insert("dup");
    cf.insert("dup");
    cf.insert("dup");
    assert_eq!(cf.len(), 3);
    assert!(cf.contains("dup"));
    cf.delete("dup");
    cf.delete("dup");
    cf.delete("dup");
    assert!(!cf.contains("dup"));
    assert_eq!(cf.len(), 0);
}

#[test]
fn default_constructor_via_with_capacity_zero() {
    let cf = CuckooFilter::with_capacity(0);
    assert!(cf.bucket_count() >= 2);
}

#[test]
fn saturation_never_produces_a_false_negative() {
    // The eviction chain runs out of moves long before 4096 keys fit in 8
    // slots. Every key the filter said yes to must still be found.
    let mut cf = CuckooFilter::with_capacity(1);
    let mut accepted = Vec::new();
    for i in 0..4096u32 {
        let key = format!("k{i}");
        if cf.insert(&key) {
            accepted.push(key);
        }
    }
    assert!(
        accepted.len() < 4096,
        "a 2-bucket filter must refuse somewhere"
    );
    for key in &accepted {
        assert!(cf.contains(key), "{key} was accepted then lost");
    }
    assert_eq!(cf.len(), accepted.len());
}

#[test]
fn insert_if_absent_suppresses_a_repeat() {
    let mut cf = CuckooFilter::with_capacity(1000);
    assert!(cf.insert_if_absent("SEQ-1"));
    assert!(
        !cf.insert_if_absent("SEQ-1"),
        "a repeat is not stored twice"
    );
    assert_eq!(cf.len(), 1);
    assert!(cf.insert_if_absent("SEQ-2"));
    assert_eq!(cf.len(), 2);
    assert!(cf.delete("SEQ-1"));
    assert!(cf.insert_if_absent("SEQ-1"), "absent again after delete");
}

#[test]
fn try_insert_reports_not_enough_space() {
    let mut cf = CuckooFilter::with_capacity(1);
    let mut err = None;
    for i in 0..4096u32 {
        if let Err(e) = cf.try_insert(&format!("k{i}")) {
            err = Some(e);
            break;
        }
    }
    assert_eq!(err, Some(CuckooError::NotEnoughSpace));
}

#[test]
fn victim_is_rehomed_once_a_delete_frees_a_slot() {
    let mut cf = CuckooFilter::with_capacity(1);
    let mut accepted = Vec::new();
    for i in 0..4096u32 {
        let key = format!("k{i}");
        if cf.insert(&key) {
            accepted.push(key);
        } else {
            break;
        }
    }
    // Saturated: the next insert is refused while the victim slot is held.
    assert!(!cf.insert("blocked"));
    assert!(cf.delete(&accepted[0]));
    // The freed slot re-homes the victim, so the filter accepts again.
    assert!(cf.insert("blocked"));
    assert!(cf.contains("blocked"));
}

#[test]
fn clear_resets_to_empty_and_keeps_geometry() {
    let mut cf = CuckooFilter::with_capacity(1000);
    let buckets = cf.bucket_count();
    for i in 0..500u32 {
        cf.insert(&format!("k{i}"));
    }
    cf.clear();
    assert!(cf.is_empty());
    assert_eq!(cf.bucket_count(), buckets);
    assert_eq!(cf.load_factor(), 0.0);
    assert!(!cf.contains("k1"));
    assert!(cf.insert("after-clear"));
}

#[test]
fn byte_and_str_apis_agree() {
    let mut cf = CuckooFilter::with_capacity(100);
    assert!(cf.insert_bytes(b"ORD-7"));
    assert!(cf.contains("ORD-7"));
    assert!(cf.contains_bytes(b"ORD-7"));
    assert!(cf.delete_bytes(b"ORD-7"));
    assert!(!cf.contains("ORD-7"));

    // Non-UTF-8 keys are legal on the byte API.
    assert!(cf.insert_bytes(&[0xff, 0x00, 0xfe]));
    assert!(cf.contains_bytes(&[0xff, 0x00, 0xfe]));
}

#[test]
fn capacity_load_factor_and_size_track_occupancy() {
    let mut cf = CuckooFilter::with_capacity(1000);
    assert_eq!(cf.capacity(), cf.bucket_count() * BUCKET_SIZE);
    assert_eq!(cf.size_in_bytes(), cf.bucket_count() * BUCKET_SIZE);
    assert_eq!(cf.load_factor(), 0.0);
    for i in 0..256u32 {
        cf.insert(&format!("k{i}"));
    }
    let expected = 256.0 / cf.capacity() as f64;
    assert!((cf.load_factor() - expected).abs() < 1e-12);
}

#[test]
fn estimated_fpp_rises_with_load_and_matches_the_closed_form() {
    let mut cf = CuckooFilter::with_capacity(1000);
    assert_eq!(cf.estimated_fpp(), 0.0);
    for i in 0..400u32 {
        cf.insert(&format!("k{i}"));
    }
    let low = cf.estimated_fpp();
    for i in 400..900u32 {
        cf.insert(&format!("k{i}"));
    }
    let high = cf.estimated_fpp();
    assert!(high > low, "fpp should rise with occupancy");

    let alpha = cf.load_factor();
    let expected = 1.0 - (1.0 - 1.0 / 256.0f64).powf(2.0 * BUCKET_SIZE as f64 * alpha);
    assert!((high - expected).abs() < 1e-12);
    // 8-bit fingerprints at four slots per bucket sit near 3% when full.
    assert!(high < 0.04, "fpp {high} above the 8-bit ceiling");
}

#[test]
fn union_merges_a_second_filter() {
    let mut a = CuckooFilter::with_capacity(1000);
    let mut b = CuckooFilter::with_capacity(1000);
    for i in 0..200u32 {
        a.insert(&format!("a{i}"));
        b.insert(&format!("b{i}"));
    }
    a.union(&b).expect("same geometry, plenty of room");
    for i in 0..200u32 {
        assert!(a.contains(&format!("a{i}")));
        assert!(a.contains(&format!("b{i}")), "b{i} lost in the merge");
    }
    assert_eq!(a.len(), 400);
}

#[test]
fn union_refuses_a_different_geometry() {
    let mut a = CuckooFilter::with_capacity(1000);
    let b = CuckooFilter::with_capacity(100_000);
    let err = a.union(&b).unwrap_err();
    assert_eq!(
        err,
        CuckooError::GeometryMismatch {
            lhs: a.bucket_count(),
            rhs: b.bucket_count()
        }
    );
    assert!(err.to_string().contains("incompatible cuckoo geometry"));
}

#[test]
fn union_refuses_when_the_target_is_full() {
    let mut a = CuckooFilter::with_capacity(1);
    let mut b = CuckooFilter::with_capacity(1);
    for i in 0..64u32 {
        a.insert(&format!("a{i}"));
        b.insert(&format!("b{i}"));
    }
    assert_eq!(a.union(&b), Err(CuckooError::NotEnoughSpace));
}

#[test]
fn serialise_round_trip_preserves_membership() {
    let mut cf = CuckooFilter::with_capacity(1000);
    for i in 0..500u32 {
        cf.insert(&format!("k{i}"));
    }
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();
    assert_eq!(buf.len(), 17 + cf.bucket_count() * BUCKET_SIZE);

    let reloaded = CuckooFilter::parse(&buf).unwrap();
    assert_eq!(reloaded.len(), cf.len());
    assert_eq!(reloaded.bucket_count(), cf.bucket_count());
    for i in 0..500u32 {
        assert!(reloaded.contains(&format!("k{i}")));
    }
}

#[test]
fn serialise_round_trip_carries_the_victim() {
    let mut cf = CuckooFilter::with_capacity(1);
    let mut accepted = Vec::new();
    for i in 0..4096u32 {
        let key = format!("k{i}");
        if cf.insert(&key) {
            accepted.push(key);
        } else {
            break;
        }
    }
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();
    let reloaded = CuckooFilter::parse(&buf).unwrap();
    for key in &accepted {
        assert!(reloaded.contains(key), "{key} lost across serialisation");
    }
}

#[test]
fn parse_rejects_malformed_input() {
    assert!(CuckooFilter::parse(&[0u8; 4]).is_err());

    let mut cf = CuckooFilter::with_capacity(100);
    cf.insert("k");
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();

    let truncated = &buf[..buf.len() - 1];
    assert!(CuckooFilter::parse(truncated).is_err());

    let mut bad_geometry = buf.clone();
    bad_geometry[0..4].copy_from_slice(&3u32.to_be_bytes());
    assert!(CuckooFilter::parse(&bad_geometry).is_err());

    let mut bad_victim = buf.clone();
    bad_victim[13..17].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(CuckooFilter::parse(&bad_victim).is_err());
}

#[test]
fn stress_insert_contains_delete_cycle() {
    let mut cf = CuckooFilter::with_capacity(2000);
    for cycle in 0..3 {
        for i in 0..1000 {
            cf.insert(&format!("cycle{cycle}-k{i}"));
        }
        for i in 0..1000 {
            assert!(cf.contains(&format!("cycle{cycle}-k{i}")));
        }
        for i in 0..1000 {
            cf.delete(&format!("cycle{cycle}-k{i}"));
        }
    }
    assert_eq!(cf.len(), 0);
}

/// Pins the exact serialised bytes. The Java port's `wireFormatFixture` test
/// asserts the same string, so a change to either encoder breaks both suites
/// rather than silently forking the format.
#[test]
fn wire_format_fixture() {
    let mut cf = CuckooFilter::with_capacity(4);
    for sym in ["AAPL", "MSFT", "GOOG"] {
        assert!(cf.insert(sym));
    }
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "000000020000000000000003000000000098000000a81a0000");
}
