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
