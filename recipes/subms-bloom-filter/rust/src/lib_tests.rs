//! Bloom filter correctness + sanity-check the false-positive rate. Colocated
//! with the module and included via `#[path]` (see `lib.rs`).

use super::*;

#[test]
fn present_keys_always_match() {
    let mut bf = BloomFilter::new(1000);
    for i in 0..1000 {
        bf.add(&format!("key{i}"));
    }
    for i in 0..1000 {
        assert!(bf.might_contain(&format!("key{i}")), "key{i} should match");
    }
}

#[test]
fn empty_filter_rejects() {
    let bf = BloomFilter::new(100);
    assert!(!bf.might_contain("anything"));
}

#[test]
fn round_trip_serialisation() {
    let mut original = BloomFilter::new(500);
    for i in 0..500 {
        original.add(&format!("k{i}"));
    }
    let mut bytes = Vec::new();
    original.write_to(&mut bytes).unwrap();

    let parsed = BloomFilter::parse(&bytes).unwrap();
    assert_eq!(parsed.bit_count(), original.bit_count());
    assert_eq!(parsed.k(), original.k());
    for i in 0..500 {
        assert!(
            parsed.might_contain(&format!("k{i}")),
            "k{i} survives round-trip"
        );
    }
}

#[test]
fn parse_rejects_short_header() {
    match BloomFilter::parse(&[0u8; 11]) {
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidData),
        Ok(_) => panic!("expected InvalidData error on short header"),
    }
}

#[test]
fn parse_rejects_truncated_bits() {
    let mut bf = BloomFilter::new(64);
    bf.add("x");
    let mut bytes = Vec::new();
    bf.write_to(&mut bytes).unwrap();
    bytes.truncate(bytes.len() - 1);
    match BloomFilter::parse(&bytes) {
        Err(e) => assert_eq!(e.kind(), std::io::ErrorKind::InvalidData),
        Ok(_) => panic!("expected InvalidData error on truncated bits"),
    }
}

#[test]
fn accessors_expose_constants() {
    let bf = BloomFilter::new(1000);
    assert_eq!(bf.k(), 7);
    assert!(bf.bit_count() >= 64);
}

/// 10 bits/key + k=7 is sized for ~1% FPR. Allow generous headroom for
/// statistical noise so this never flakes (5% threshold; typical run ~1%).
#[test]
fn false_positive_rate_is_roughly_1_percent() {
    let n = 10_000;
    let mut bf = BloomFilter::new(n);
    for i in 0..n {
        bf.add(&format!("present{i}"));
    }
    let mut false_positives = 0usize;
    let probes = 100_000;
    for i in 0..probes {
        if bf.might_contain(&format!("absent{i}")) {
            false_positives += 1;
        }
    }
    let fpr = false_positives as f64 / probes as f64;
    assert!(
        fpr < 0.05,
        "fpr {fpr:.4} too high (expected ~0.01 with 10 bits/key + k=7)"
    );
}

#[test]
fn zero_expected_entries_yields_minimum_filter() {
    let bf = BloomFilter::new(0);
    assert!(bf.bit_count() >= 1, "bit_count >= 1 even at n=0");
    assert!(bf.k() >= 1, "k >= 1 even at n=0");
    assert!(
        !bf.might_contain("anything"),
        "empty filter rejects every probe"
    );
}

#[test]
fn duplicate_add_is_idempotent() {
    let mut bf1 = BloomFilter::new(100);
    bf1.add("key");
    let mut bytes = Vec::new();
    bf1.write_to(&mut bytes).expect("write");
    let after_one = bytes.clone();
    bf1.add("key");
    bytes.clear();
    bf1.write_to(&mut bytes).expect("write");
    assert_eq!(
        after_one, bytes,
        "second add of same key must not change bits"
    );
}

#[test]
fn parse_rejects_truncated_input() {
    let result = BloomFilter::parse(&[1u8, 2, 3]);
    assert!(result.is_err(), "truncated header should produce an error");
}

#[test]
fn long_unicode_key_accepted() {
    let mut bf = BloomFilter::new(100);
    let s = "hello-rocket-very-long-string-with-mixed-content-123456789";
    bf.add(s);
    assert!(bf.might_contain(s));
}

/// The bytes a 64-bit, k=7 filter holding {alice, bob, carol} serialises to.
/// The Java suite pins the identical string. This is what makes the wire
/// format a cross-language contract rather than a claim: it caught the Java
/// port probing `Math.floorMod` where Rust probes an unsigned remainder,
/// which silently disagreed on ~46% of probe positions.
const CROSS_LANG_FIXTURE: &str = "000000400000000700000001210c6708c21200c4";

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

#[test]
fn wire_format_matches_cross_language_fixture() {
    let mut bf = BloomFilter::new(4);
    for key in ["alice", "bob", "carol"] {
        bf.add(key);
    }
    let mut bytes = Vec::new();
    bf.write_to(&mut bytes).unwrap();
    assert_eq!(to_hex(&bytes), CROSS_LANG_FIXTURE);
}

#[test]
fn cross_language_fixture_parses_back_to_its_members() {
    let bytes: Vec<u8> = (0..CROSS_LANG_FIXTURE.len() / 2)
        .map(|i| u8::from_str_radix(&CROSS_LANG_FIXTURE[i * 2..i * 2 + 2], 16).unwrap())
        .collect();
    let bf = BloomFilter::parse(&bytes).unwrap();
    assert_eq!(bf.bit_count(), 64);
    assert_eq!(bf.k(), 7);
    for key in ["alice", "bob", "carol"] {
        assert!(bf.might_contain(key), "{key} survives the fixture");
    }
}

#[test]
fn clear_empties_the_filter_and_keeps_geometry() {
    let mut bf = BloomFilter::new(1000);
    for i in 0..1000 {
        bf.add(&format!("key{i}"));
    }
    let (m, k) = (bf.bit_count(), bf.k());
    bf.clear();
    assert_eq!(bf.set_bits(), 0);
    assert_eq!((bf.bit_count(), bf.k()), (m, k));
    assert!(!bf.might_contain("key0"));
}

#[test]
fn union_is_the_same_filter_as_one_built_from_both_key_sets() {
    let mut left = BloomFilter::new(1000);
    let mut right = BloomFilter::new(1000);
    let mut both = BloomFilter::new(1000);
    for i in 0..500 {
        left.add(&format!("l{i}"));
        both.add(&format!("l{i}"));
        right.add(&format!("r{i}"));
        both.add(&format!("r{i}"));
    }
    left.union(&right).unwrap();

    let mut merged_bytes = Vec::new();
    left.write_to(&mut merged_bytes).unwrap();
    let mut both_bytes = Vec::new();
    both.write_to(&mut both_bytes).unwrap();
    assert_eq!(merged_bytes, both_bytes);

    for i in 0..500 {
        assert!(left.might_contain(&format!("l{i}")));
        assert!(left.might_contain(&format!("r{i}")));
    }
}

#[test]
fn union_refuses_mismatched_geometry() {
    let mut small = BloomFilter::new(100);
    let big = BloomFilter::new(1000);
    assert!(!small.is_compatible(&big));
    let err = small.union(&big).unwrap_err();
    assert_eq!(err.lhs.1, 7);
    assert!(err.to_string().contains("incompatible bloom geometry"));
}

#[test]
fn empty_filter_reports_no_occupancy() {
    let bf = BloomFilter::new(10_000);
    assert_eq!(bf.set_bits(), 0);
    assert_eq!(bf.approximate_element_count(), 0);
    assert_eq!(bf.estimated_fpp(), 0.0);
}

#[test]
fn approximate_element_count_tracks_actual_cardinality() {
    let n = 5_000;
    let mut bf = BloomFilter::new(n);
    for i in 0..n {
        bf.add(&format!("key{i}"));
    }
    let est = bf.approximate_element_count() as f64;
    let err = (est - n as f64).abs() / n as f64;
    assert!(err < 0.05, "estimate {est} off by {err:.3} from {n}");
}

#[test]
fn estimated_fpp_rises_as_the_filter_saturates() {
    let mut bf = BloomFilter::new(1_000);
    for i in 0..1_000 {
        bf.add(&format!("key{i}"));
    }
    let at_design_point = bf.estimated_fpp();
    for i in 1_000..10_000 {
        bf.add(&format!("key{i}"));
    }
    assert!(
        bf.estimated_fpp() > at_design_point,
        "overfilling must raise the estimate ({at_design_point} -> {})",
        bf.estimated_fpp()
    );
    assert!(at_design_point < 0.05, "design point stays near 1%");
}

#[test]
fn saturated_filter_reports_an_unusable_element_count() {
    let mut bf = BloomFilter::new(0);
    for i in 0..10_000 {
        bf.add(&format!("key{i}"));
    }
    assert_eq!(bf.set_bits(), bf.bit_count() as u64);
    assert_eq!(bf.approximate_element_count(), u64::MAX);
}
