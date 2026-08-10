use super::*;

#[test]
fn round_trip_below_saturation() {
    let mut cf = CompressedCuckooFilter::with_capacity(1000);
    for i in 0..500u32 {
        assert!(cf.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.contains(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.delete(&format!("k{i}")));
    }
    assert_eq!(cf.len(), 0);
}

#[test]
fn empty_filter_rejects_everything() {
    let cf = CompressedCuckooFilter::with_capacity(100);
    assert!(!cf.contains("never-inserted"));
    assert!(cf.is_empty());
    assert_eq!(cf.len(), 0);
}

#[test]
fn occupied_bytes_grows_with_inserts() {
    let mut cf = CompressedCuckooFilter::with_capacity(500);
    let baseline = cf.occupied_bytes();
    for i in 0..200u32 {
        cf.insert(&format!("k{i}"));
    }
    assert!(cf.occupied_bytes() > baseline, "expected occupancy to grow");
}

#[test]
fn delete_unknown_returns_false() {
    let mut cf = CompressedCuckooFilter::with_capacity(100);
    cf.insert("known");
    assert!(!cf.delete("never-inserted"));
    assert!(cf.contains("known"));
}

#[test]
fn sorted_invariant_holds_through_inserts_and_deletes() {
    // Whitebox: after mixed ops every bucket's run must be sorted
    // ascending.
    let mut cf = CompressedCuckooFilter::with_capacity(500);
    for i in 0..400u32 {
        cf.insert(&format!("k{i}"));
    }
    for i in 0..200u32 {
        cf.delete(&format!("k{i}"));
    }
    for i in 400..500u32 {
        cf.insert(&format!("k{i}"));
    }
    for bucket in &cf.buckets {
        let count = bucket[0] as usize;
        for k in 1..count {
            assert!(
                bucket[1 + k - 1] <= bucket[1 + k],
                "bucket out of order: {bucket:?}"
            );
        }
    }
}

#[test]
fn false_positive_rate_in_three_percent_range() {
    let n = 5_000usize;
    let mut cf = CompressedCuckooFilter::with_capacity(n);
    for i in 0..n {
        cf.insert(&format!("present{i}"));
    }
    let probes = 10_000usize;
    let mut fp = 0usize;
    for i in 0..probes {
        if cf.contains(&format!("absent{i}")) {
            fp += 1;
        }
    }
    let fpr = fp as f64 / probes as f64;
    assert!(fpr < 0.03, "fpr {fpr:.4} too high");
}

#[test]
fn bucket_count_is_power_of_two() {
    let cf = CompressedCuckooFilter::with_capacity(1000);
    assert!(cf.bucket_count().is_power_of_two());
}

#[test]
fn duplicate_inserts_stack_in_bucket() {
    let mut cf = CompressedCuckooFilter::with_capacity(100);
    cf.insert("dup");
    cf.insert("dup");
    cf.insert("dup");
    assert_eq!(cf.len(), 3);
    assert!(cf.contains("dup"));
    cf.delete("dup");
    cf.delete("dup");
    cf.delete("dup");
    assert!(!cf.contains("dup"));
}

#[test]
fn saturation_never_produces_a_false_negative() {
    let mut cf = CompressedCuckooFilter::with_capacity(1);
    let mut accepted = Vec::new();
    for i in 0..4096u32 {
        let key = format!("k{i}");
        if cf.insert(&key) {
            accepted.push(key);
        }
    }
    assert!(accepted.len() < 4096, "a 2-bucket filter must refuse");
    for key in &accepted {
        assert!(cf.contains(key), "{key} was accepted then lost");
    }
}

#[test]
fn victim_is_rehomed_once_a_delete_frees_a_slot() {
    let mut cf = CompressedCuckooFilter::with_capacity(1);
    let mut accepted = Vec::new();
    for i in 0..4096u32 {
        let key = format!("k{i}");
        if cf.insert(&key) {
            accepted.push(key);
        } else {
            break;
        }
    }
    assert!(!cf.insert("blocked"));
    assert!(cf.delete(&accepted[0]));
    assert!(cf.insert("blocked"));
    assert!(cf.contains("blocked"));
}

#[test]
fn clear_resets_to_empty_and_keeps_geometry() {
    let mut cf = CompressedCuckooFilter::with_capacity(1000);
    let buckets = cf.bucket_count();
    for i in 0..300u32 {
        cf.insert(&format!("k{i}"));
    }
    cf.clear();
    assert!(cf.is_empty());
    assert_eq!(cf.bucket_count(), buckets);
    assert_eq!(cf.occupied_bytes(), buckets);
    assert!(!cf.contains("k1"));
}

#[test]
fn compact_serialisation_round_trip() {
    let mut cf = CompressedCuckooFilter::with_capacity(10_000);
    for i in 0..3_000u32 {
        cf.insert(&format!("k{i}"));
    }
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();
    // The whole point of the feature: the stream is the live bytes, not the
    // base layout's fixed four slots per bucket.
    assert_eq!(buf.len(), 17 + cf.occupied_bytes());
    assert!(buf.len() < 17 + cf.bucket_count() * BUCKET_SIZE);

    let reloaded = CompressedCuckooFilter::parse(&buf).unwrap();
    assert_eq!(reloaded.len(), cf.len());
    assert_eq!(reloaded.bucket_count(), cf.bucket_count());
    for i in 0..3_000u32 {
        assert!(reloaded.contains(&format!("k{i}")));
    }
}

#[test]
fn compact_parse_rejects_malformed_input() {
    assert!(CompressedCuckooFilter::parse(&[0u8; 4]).is_err());

    let mut cf = CompressedCuckooFilter::with_capacity(100);
    cf.insert("k");
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();

    assert!(CompressedCuckooFilter::parse(&buf[..buf.len() - 1]).is_err());

    let mut bad_geometry = buf.clone();
    bad_geometry[0..4].copy_from_slice(&3u32.to_be_bytes());
    assert!(CompressedCuckooFilter::parse(&bad_geometry).is_err());

    let mut bad_victim = buf.clone();
    bad_victim[13..17].copy_from_slice(&u32::MAX.to_be_bytes());
    assert!(CompressedCuckooFilter::parse(&bad_victim).is_err());

    let mut bad_run = buf.clone();
    bad_run[17] = 99; // a count byte beyond BUCKET_SIZE
    assert!(CompressedCuckooFilter::parse(&bad_run).is_err());
}

/// Pins the compact bytes. The Java port's `compactWireFormatFixture` asserts
/// the same string.
#[test]
fn compact_wire_format_fixture() {
    let mut cf = CompressedCuckooFilter::with_capacity(4);
    for sym in ["AAPL", "MSFT", "GOOG"] {
        assert!(cf.insert(sym));
    }
    let mut buf = Vec::new();
    cf.write_to(&mut buf).unwrap();
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(hex, "00000002000000000000000300000000000198021aa8");
}
