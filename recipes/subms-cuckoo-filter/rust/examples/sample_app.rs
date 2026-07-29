//! Sample app: a tour of `subms-cuckoo-filter`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features dynamic`) to see the feature
//! sections light up.
//!
//! * base                 - a live-order membership set where fills delete entries
//! * variable-fingerprint - widen the fingerprint to cut false positives on a risk pre-check
//! * dynamic              - an intraday dedup window that grows past its initial sizing
//! * concurrent-reads     - a reader fleet fanning out over a frozen snapshot
//! * compressed-buckets   - a smaller serialized footprint at moderate load

use subms_cuckoo_filter::CuckooFilter;

fn main() {
    base_open_order_set();

    #[cfg(feature = "variable-fingerprint")]
    variable_fingerprint_risk_precheck();

    #[cfg(feature = "dynamic")]
    dynamic_dedup_window();

    #[cfg(feature = "concurrent-reads")]
    concurrent_reads_market_fanout();

    #[cfg(feature = "compressed-buckets")]
    compressed_persistence();
}

/// Base API: an order-management system keeps the set of currently-open client
/// order IDs. A `contains` is the cheap pre-check before the authoritative
/// book lookup; a fill or cancel `delete`s the ID. That delete is the move a
/// bloom filter cannot make - a filled order has to leave the live set.
fn base_open_order_set() {
    println!("== base: open-order membership set ==");
    let mut open = CuckooFilter::with_capacity(10_000);
    for oid in ["ORD-1001", "ORD-1002", "ORD-1003", "ORD-1004"] {
        open.insert(oid);
    }
    println!("  live orders: {}", open.len());

    assert!(open.contains("ORD-1002"));
    open.delete("ORD-1002"); // a fill closes it out
    println!("  after fill on ORD-1002 -> still live? {}", open.contains("ORD-1002"));

    assert!(!open.contains("ORD-1002"), "a filled order leaves the live set");
    assert!(open.contains("ORD-1001"), "no false negatives for still-open orders");
    assert_eq!(open.len(), 3);
}

/// `variable-fingerprint` feature: a false positive on this pre-check fires a
/// costly authoritative risk lookup for a symbol that was never restricted.
/// Widening the fingerprint from 8 to 16 bits shrinks that rate by orders of
/// magnitude, paying an extra byte per slot.
#[cfg(feature = "variable-fingerprint")]
fn variable_fingerprint_risk_precheck() {
    use subms_cuckoo_filter::{FingerprintWidth, VariableFpCuckooFilter};
    println!("\n== variable-fingerprint: cut false positives on a risk pre-check ==");
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
    println!("  8-bit false positives:  {narrow_fp}");
    println!("  16-bit false positives: {wide_fp}");
    assert!(wide_fp < narrow_fp, "wider fingerprint lowers the false-positive rate");
}

/// `dynamic` feature: a session dedup window for inbound message IDs whose
/// volume is not known when the day opens. The base filter rejects inserts at
/// saturation; the dynamic variant chains a fresh layer as load climbs, so a
/// late-session ID is never dropped.
#[cfg(feature = "dynamic")]
fn dynamic_dedup_window() {
    use subms_cuckoo_filter::DynamicCuckooFilter;
    println!("\n== dynamic: an intraday dedup window that grows itself ==");
    let mut seen = DynamicCuckooFilter::with_threshold(1_000, 0.5);
    for i in 0..20_000u32 {
        seen.insert(&format!("MSG-{i}"));
    }
    println!(
        "  20k ids -> {} layers, active load {:.2}",
        seen.layer_count(),
        seen.load_factor()
    );
    assert!(seen.layer_count() > 1, "the window grew past its initial sizing");
    for i in 0..20_000u32 {
        assert!(seen.contains(&format!("MSG-{i}")), "no id dropped as the window grew");
    }
}

/// `concurrent-reads` feature: the matching engine is the single writer; a
/// fleet of pricing/risk readers fans out lock-free over a frozen
/// `Arc<CuckooSnapshot>` of the open-order set. The snapshot is an eager copy,
/// so writes after the capture never disturb a reader mid-scan.
#[cfg(feature = "concurrent-reads")]
fn concurrent_reads_market_fanout() {
    use std::sync::Arc;
    use subms_cuckoo_filter::CuckooSnapshot;
    println!("\n== concurrent-reads: a reader fleet over a frozen open-order set ==");
    let mut open = CuckooFilter::with_capacity(10_000);
    for i in 0..1_000u32 {
        open.insert(&format!("ORD-{i}"));
    }
    let snap = CuckooSnapshot::capture(&open);

    // The writer keeps mutating after the snapshot is frozen.
    open.insert("ORD-LATE");
    open.delete("ORD-0");

    let mut handles = Vec::new();
    for _ in 0..4 {
        let s = Arc::clone(&snap);
        handles.push(std::thread::spawn(move || {
            (0..1_000u32).filter(|i| s.contains(&format!("ORD-{i}"))).count()
        }));
    }
    for h in handles {
        assert_eq!(h.join().unwrap(), 1_000, "every reader sees the whole frozen set");
    }
    println!("  4 readers each matched all 1000 orders in the snapshot");
    assert!(snap.contains("ORD-0"), "snapshot keeps its pre-freeze state");
    assert!(!snap.contains("ORD-LATE"), "snapshot does not see the writer's later insert");
}

/// `compressed-buckets` feature: persisting or replicating the filter. The
/// sorted-run encoding stores only the live fingerprints plus a count byte, so
/// the serialized footprint at the low-to-moderate load where filters actually
/// run beats the base filter's fixed four-slot buckets.
#[cfg(feature = "compressed-buckets")]
fn compressed_persistence() {
    use subms_cuckoo_filter::CompressedCuckooFilter;
    println!("\n== compressed-buckets: smaller serialized footprint at moderate load ==");
    let mut cf = CompressedCuckooFilter::with_capacity(10_000);
    for i in 0..3_000u32 {
        cf.insert(&format!("ORD-{i}"));
    }
    let base_fixed_bytes = cf.bucket_count() * 4; // base layout: 4 slot bytes per bucket
    println!(
        "  live bytes {}, base fixed-array bytes {}",
        cf.occupied_bytes(),
        base_fixed_bytes
    );
    assert!(
        cf.occupied_bytes() < base_fixed_bytes,
        "sorted-run encoding wins at moderate load"
    );
    for i in 0..3_000u32 {
        assert!(cf.contains(&format!("ORD-{i}")));
    }
    assert!(cf.delete("ORD-0"), "delete still works on the compressed layout");
}
