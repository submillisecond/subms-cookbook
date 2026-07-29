use super::*;

#[test]
fn round_trip_eight_bit() {
    let mut cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Eight);
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
fn round_trip_twelve_bit() {
    let mut cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Twelve);
    for i in 0..500u32 {
        assert!(cf.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.contains(&format!("k{i}")));
    }
    assert_eq!(cf.len(), 500);
}

#[test]
fn round_trip_sixteen_bit() {
    let mut cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Sixteen);
    for i in 0..500u32 {
        assert!(cf.insert(&format!("k{i}")));
    }
    for i in 0..500u32 {
        assert!(cf.contains(&format!("k{i}")));
    }
}

#[test]
fn wider_fingerprint_lowers_fpr() {
    // Same insert + probe set across both widths; 16-bit must be
    // strictly fewer false positives than 8-bit at this volume.
    let n = 5_000usize;
    let probes = 10_000usize;

    let mut narrow = VariableFpCuckooFilter::new(n, FingerprintWidth::Eight);
    let mut wide = VariableFpCuckooFilter::new(n, FingerprintWidth::Sixteen);
    for i in 0..n {
        narrow.insert(&format!("present{i}"));
        wide.insert(&format!("present{i}"));
    }
    let mut narrow_fp = 0usize;
    let mut wide_fp = 0usize;
    for i in 0..probes {
        let k = format!("absent{i}");
        if narrow.contains(&k) {
            narrow_fp += 1;
        }
        if wide.contains(&k) {
            wide_fp += 1;
        }
    }
    assert!(
        wide_fp < narrow_fp,
        "wide_fp={wide_fp} should be < narrow_fp={narrow_fp}"
    );
}

#[test]
fn empty_filter_rejects_everything() {
    let cf = VariableFpCuckooFilter::new(100, FingerprintWidth::Twelve);
    assert!(!cf.contains("never-inserted"));
    assert!(cf.is_empty());
}

#[test]
fn width_accessor_reports_configured_value() {
    let cf = VariableFpCuckooFilter::new(100, FingerprintWidth::Twelve);
    assert_eq!(cf.width(), FingerprintWidth::Twelve);
    assert_eq!(cf.width().bits(), 12);
}

#[test]
fn bits_maps_each_width() {
    assert_eq!(FingerprintWidth::Eight.bits(), 8);
    assert_eq!(FingerprintWidth::Twelve.bits(), 12);
    assert_eq!(FingerprintWidth::Sixteen.bits(), 16);
}

#[test]
fn delete_unknown_is_false() {
    let mut cf = VariableFpCuckooFilter::new(100, FingerprintWidth::Sixteen);
    assert!(!cf.delete("never-inserted"));
}

#[test]
fn bucket_count_is_power_of_two() {
    let cf = VariableFpCuckooFilter::new(1000, FingerprintWidth::Twelve);
    let n = cf.bucket_count();
    assert!(n.is_power_of_two(), "{n} not power of two");
}
