use super::*;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Pinned so the Java port has something to fail against. Same key set, same
/// precision, same bytes - the whole point of the format.
pub(crate) const FIXTURE_KEYS: [&str; 5] = ["AAPL", "MSFT", "NVDA", "TSLA", "AMZN"];
pub(crate) const FIXTURE_P: u32 = 4;
pub(crate) const DENSE_FIXTURE_HEX: &str = "53484c4c0100040000010000000000000000010001030003";
#[cfg(feature = "sparse")]
const SPARSE_FIXTURE_HEX: &str =
    "53484c4c0101040000000040000000050000000c010000000a010000000f0300000001010000000d03";

fn fixture() -> HyperLogLog {
    let mut hll = HyperLogLog::new(FIXTURE_P);
    for k in FIXTURE_KEYS {
        hll.add(k);
    }
    hll
}

#[test]
fn dense_wire_bytes_are_pinned() {
    // If this changes, the Java port's identical fixture must change with it -
    // and that is a format break, not a refactor.
    assert_eq!(hex(&fixture().to_bytes()), DENSE_FIXTURE_HEX);
}

#[test]
fn dense_round_trip_preserves_registers() {
    let hll = fixture();
    let back = HyperLogLog::from_bytes(&hll.to_bytes()).unwrap();
    assert_eq!(back.precision(), hll.precision());
    assert_eq!(back.registers(), hll.registers());
    assert_eq!(back.estimate(), hll.estimate());
}

#[test]
fn dense_round_trip_at_working_precision() {
    let mut hll = HyperLogLog::new(14);
    for i in 0..20_000u32 {
        hll.add_u64(u64::from(i));
    }
    let bytes = hll.to_bytes();
    assert_eq!(bytes.len(), 8 + 16384);
    let back = HyperLogLog::from_bytes(&bytes).unwrap();
    assert_eq!(back.estimate(), hll.estimate());
}

#[test]
fn rejects_foreign_magic() {
    let mut bytes = fixture().to_bytes();
    bytes[0] = b'X';
    assert_eq!(HyperLogLog::from_bytes(&bytes), Err(HllError::BadMagic));
}

#[test]
fn rejects_future_version() {
    let mut bytes = fixture().to_bytes();
    bytes[4] = 2;
    assert_eq!(
        HyperLogLog::from_bytes(&bytes),
        Err(HllError::UnsupportedVersion(2))
    );
}

#[test]
fn rejects_unknown_encoding() {
    let mut bytes = fixture().to_bytes();
    bytes[5] = 9;
    assert_eq!(
        HyperLogLog::from_bytes(&bytes),
        Err(HllError::UnsupportedEncoding(9))
    );
}

#[test]
fn rejects_out_of_range_precision() {
    let mut bytes = fixture().to_bytes();
    bytes[6] = 31;
    assert_eq!(
        HyperLogLog::from_bytes(&bytes),
        Err(HllError::InvalidPrecision(31))
    );
}

#[test]
fn rejects_truncated_header_and_payload() {
    let bytes = fixture().to_bytes();
    assert_eq!(
        HyperLogLog::from_bytes(&bytes[..3]),
        Err(HllError::Truncated {
            expected: 8,
            actual: 3
        })
    );
    assert_eq!(
        HyperLogLog::from_bytes(&bytes[..12]),
        Err(HllError::Truncated {
            expected: 24,
            actual: 12
        })
    );
}

#[test]
fn decoded_sketch_merges_with_a_live_one() {
    let mut shard_a = HyperLogLog::new(12);
    let mut shard_b = HyperLogLog::new(12);
    for i in 0..4_000u64 {
        shard_a.add_u64(i);
        shard_b.add_u64(i + 2_000);
    }
    // Only the bytes cross the boundary, never the ids.
    let shipped = HyperLogLog::from_bytes(&shard_b.to_bytes()).unwrap();
    shard_a.merge(&shipped).unwrap();
    let est = shard_a.estimate();
    assert!(
        (est - 6_000.0).abs() / 6_000.0 < 0.05,
        "6000 distinct across two shards, got {est}"
    );
}

#[cfg(feature = "sparse")]
mod sparse_wire {
    use super::*;
    use crate::SparseHyperLogLog;

    #[test]
    fn sparse_wire_bytes_are_pinned() {
        let mut s = SparseHyperLogLog::with_threshold(FIXTURE_P, 64);
        for k in FIXTURE_KEYS {
            s.add(k);
        }
        assert!(s.is_sparse());
        assert_eq!(hex(&s.to_bytes()), SPARSE_FIXTURE_HEX);
    }

    #[test]
    fn sparse_round_trip_keeps_shape_and_threshold() {
        let mut s = SparseHyperLogLog::with_threshold(12, 64);
        for i in 0..30u64 {
            s.add_u64(i);
        }
        let back = SparseHyperLogLog::from_bytes(&s.to_bytes()).unwrap();
        assert!(back.is_sparse());
        assert_eq!(back.threshold(), 64);
        assert_eq!(back.entry_count(), s.entry_count());
        assert_eq!(back.estimate(), s.estimate());
    }

    #[test]
    fn promoted_sparse_writes_the_dense_encoding() {
        let mut s = SparseHyperLogLog::with_threshold(10, 8);
        for i in 0..50u64 {
            s.add_u64(i);
        }
        assert!(!s.is_sparse());
        let bytes = s.to_bytes();
        assert_eq!(bytes[5], 0, "dense encoding byte");
        let back = SparseHyperLogLog::from_bytes(&bytes).unwrap();
        assert!(
            !back.is_sparse(),
            "a promoted writer yields a promoted reader"
        );
        assert_eq!(back.estimate(), s.estimate());
    }

    #[test]
    fn sparse_reader_rejects_a_truncated_entry_list() {
        let mut s = SparseHyperLogLog::with_threshold(12, 64);
        for i in 0..10u64 {
            s.add_u64(i);
        }
        let bytes = s.to_bytes();
        let cut = bytes.len() - 3;
        assert!(matches!(
            SparseHyperLogLog::from_bytes(&bytes[..cut]),
            Err(HllError::Truncated { .. })
        ));
    }

    #[test]
    fn dense_reader_refuses_a_sparse_buffer() {
        let mut s = SparseHyperLogLog::with_threshold(12, 64);
        s.add("one");
        assert_eq!(
            HyperLogLog::from_bytes(&s.to_bytes()),
            Err(HllError::UnsupportedEncoding(1))
        );
    }
}
