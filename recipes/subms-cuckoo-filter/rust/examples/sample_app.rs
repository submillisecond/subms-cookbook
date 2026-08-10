//! Sample app: a miniature order-management gateway built on
//! `subms-cuckoo-filter`, then a tour of each optional feature. Run the base
//! with `cargo run --example sample_app`; add `--all-features` (or a subset
//! like `--features dynamic`) to see the feature sections light up.
//!
//! * base                 - an OMS live-order set driven by a drop-copy event stream
//! * base                 - checkpoint, shard fan-in and session roll on the same set
//! * variable-fingerprint - widen the fingerprint to cut false positives on a risk pre-check
//! * dynamic              - an intraday dedup window that grows past its initial sizing
//! * concurrent-reads     - a reader fleet fanning out over a frozen snapshot
//! * compressed-buckets   - a smaller serialized footprint at moderate load

use subms_cuckoo_filter::CuckooFilter;

/// One line off a drop-copy stream. The gateway only needs to know whether an
/// id is live, so a fingerprint set stands in for the order table.
enum Event<'a> {
    New(&'a str),
    Fill(&'a str),
    Cancel(&'a str),
}

fn main() {
    let mut open = oms_gateway();
    checkpoint_and_restore(&open);
    shard_fan_in();
    session_roll(&mut open);

    #[cfg(feature = "variable-fingerprint")]
    variable_fingerprint_risk_precheck();

    #[cfg(feature = "dynamic")]
    dynamic_dedup_window();

    #[cfg(feature = "concurrent-reads")]
    concurrent_reads_market_fanout();

    #[cfg(feature = "compressed-buckets")]
    compressed_persistence();
}

/// The system: an order gateway replays a drop-copy stream into a live-order
/// set. Every inbound amend is gated on `contains` before it costs an
/// authoritative book lookup; a fill or cancel `delete`s the id. The delete is
/// the move a bloom filter cannot make, and without it the set would grow all
/// session and every closed order would keep answering yes.
fn oms_gateway() -> CuckooFilter {
    println!("== OMS gateway: live-order set from a drop-copy stream ==");
    let stream = [
        Event::New("ORD-1001"),
        Event::New("ORD-1002"),
        Event::New("ORD-1003"),
        Event::New("ORD-1004"),
        Event::Fill("ORD-1002"),
        Event::New("ORD-1005"),
        Event::Cancel("ORD-1003"),
        Event::Fill("ORD-1005"),
        Event::New("ORD-1004"), // the session resend replays one we already hold
    ];

    let mut open = CuckooFilter::with_capacity(10_000);
    let (mut opened, mut replayed, mut closed) = (0u32, 0u32, 0u32);
    for event in &stream {
        match event {
            // insert_if_absent makes the replay idempotent: a resent NEW for a
            // live order must not add a second fingerprint.
            Event::New(id) => {
                if open.insert_if_absent(id) {
                    opened += 1;
                } else {
                    replayed += 1;
                }
            }
            Event::Fill(id) | Event::Cancel(id) => {
                if open.delete(id) {
                    closed += 1;
                }
            }
        }
    }

    println!(
        "  {opened} new, {replayed} replayed, {closed} closed -> {} live",
        open.len()
    );
    println!(
        "  load {:.4}, false-positive rate {:.6}",
        open.load_factor(),
        open.estimated_fpp()
    );

    let amend = "ORD-1002";
    println!(
        "  amend for {amend} -> {}",
        if open.contains(amend) {
            "book lookup"
        } else {
            "reject, already closed"
        }
    );

    assert_eq!(open.len(), 2);
    assert!(!open.contains("ORD-1002"), "a filled order leaves the set");
    assert!(
        open.contains("ORD-1001"),
        "no false negative on a live order"
    );
    open
}

/// Checkpoint the live set to bytes and reload it. A gateway restarting
/// mid-session rebuilds membership from the last checkpoint instead of
/// replaying the whole day's drop copy.
fn checkpoint_and_restore(open: &CuckooFilter) {
    println!("\n== checkpoint: serialise the live set and reload it ==");
    let mut buf = Vec::new();
    open.write_to(&mut buf).expect("in-memory write");
    let restored = CuckooFilter::parse(&buf).expect("round trip");
    println!(
        "  {} bytes on the wire, {} live orders restored",
        buf.len(),
        restored.len()
    );
    assert!(restored.contains("ORD-1001"));
    assert_eq!(restored.len(), open.len());
}

/// Fan-in: two gateway shards each hold their own live-order set, and the
/// surveillance process merges them into one. `union` re-places every
/// fingerprint rather than OR-ing bit arrays, so both filters must share a
/// geometry - which is why both are built with the same capacity.
fn shard_fan_in() {
    println!("\n== fan-in: merge two shards' live-order sets ==");
    let mut shard_a = CuckooFilter::with_capacity(10_000);
    let mut shard_b = CuckooFilter::with_capacity(10_000);
    for i in 0..500u32 {
        shard_a.insert(&format!("A-ORD-{i}"));
        shard_b.insert(&format!("B-ORD-{i}"));
    }
    shard_a.union(&shard_b).expect("same geometry");
    println!("  merged set holds {} orders", shard_a.len());
    assert!(shard_a.contains("A-ORD-7"));
    assert!(shard_a.contains("B-ORD-7"));

    let mismatched = CuckooFilter::with_capacity(1_000_000);
    println!(
        "  merging a differently-sized shard -> {:?}",
        shard_a.union(&mismatched)
    );
}

/// Session roll: `clear` zeroes the set at the close and keeps the allocation,
/// so tomorrow's first order does not pay for a fresh 16 KB array.
fn session_roll(open: &mut CuckooFilter) {
    println!("\n== session roll: clear and reuse the allocation ==");
    let bytes = open.size_in_bytes();
    open.clear();
    println!(
        "  after close: {} live, {} bytes still held",
        open.len(),
        bytes
    );
    assert!(open.is_empty());
    assert!(!open.contains("ORD-1001"));
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
    assert!(
        wide_fp < narrow_fp,
        "wider fingerprint lowers the false-positive rate"
    );
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
    assert!(
        seen.layer_count() > 1,
        "the window grew past its initial sizing"
    );
    for i in 0..20_000u32 {
        assert!(
            seen.contains(&format!("MSG-{i}")),
            "no id dropped as the window grew"
        );
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
            (0..1_000u32)
                .filter(|i| s.contains(&format!("ORD-{i}")))
                .count()
        }));
    }
    for h in handles {
        assert_eq!(
            h.join().unwrap(),
            1_000,
            "every reader sees the whole frozen set"
        );
    }
    println!("  4 readers each matched all 1000 orders in the snapshot");
    assert!(
        snap.contains("ORD-0"),
        "snapshot keeps its pre-freeze state"
    );
    assert!(
        !snap.contains("ORD-LATE"),
        "snapshot does not see the writer's later insert"
    );
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
    let mut buf = Vec::new();
    cf.write_to(&mut buf).expect("in-memory write");
    println!(
        "  serialised {} bytes, base fixed-array layout would be {}",
        buf.len(),
        base_fixed_bytes + 17
    );
    let reloaded = CompressedCuckooFilter::parse(&buf).expect("round trip");
    assert_eq!(reloaded.len(), cf.len());
    assert!(
        cf.occupied_bytes() < base_fixed_bytes,
        "sorted-run encoding wins at moderate load"
    );
    for i in 0..3_000u32 {
        assert!(cf.contains(&format!("ORD-{i}")));
    }
    assert!(
        cf.delete("ORD-0"),
        "delete still works on the compressed layout"
    );
}
