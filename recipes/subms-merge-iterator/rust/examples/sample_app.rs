//! Sample app: a tour of `subms-merge-iterator`, base API first, then each
//! optional feature. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features dedup`) to see the feature
//! sections light up.
//!
//! The framing is an LSM-backed market-data store. The base merge consolidates
//! per-venue sorted trade streams into one time-ordered tape; each feature is a
//! piece of the read path over that store.
//!
//! * base       - consolidate per-venue sorted trade timestamps into one tape
//! * seek-to    - skip the pre-market ticks, start the scan at the session open
//! * tombstones - an instrument-reference read where a delisting shadows the row
//! * dedup      - compact append-only price shards to the freshest price per symbol
//! * priority   - a live memtable outranks stale on-disk SSTable levels on a key tie

use subms_merge_iterator::MergeIterator;

fn main() {
    base_consolidated_tape();

    #[cfg(feature = "seek-to")]
    seek_to_session_open();

    #[cfg(feature = "tombstones")]
    tombstone_reference_read();

    #[cfg(feature = "dedup")]
    dedup_price_shards();

    #[cfg(feature = "priority")]
    priority_memtable_wins();
}

/// Base API: three venues each publish trades already sorted by exchange
/// timestamp. Merging their heads on a min-heap yields one chronological
/// consolidated tape without materialising and re-sorting the union.
fn base_consolidated_tape() {
    println!("== base: consolidated trade tape ==");
    // Nanoseconds since the session epoch, per venue, each ascending.
    let venues: Vec<std::vec::IntoIter<i64>> = vec![
        vec![100, 450, 780].into_iter(),
        vec![120, 460, 810].into_iter(),
        vec![90, 300, 900].into_iter(),
    ];
    let tape: Vec<i64> = MergeIterator::new(venues).collect();
    println!("  merged {} trades: {:?}", tape.len(), tape);
    assert_eq!(tape.len(), 9, "every trade appears once");
    assert!(
        tape.windows(2).all(|w| w[0] <= w[1]),
        "tape is chronological"
    );
    assert_eq!(tape.first(), Some(&90), "earliest trade leads the tape");
}

/// `seek-to` feature: a session scan wants the regular-session trades only.
/// `seek(open)` advances every venue past its pre-market ticks in one bounded
/// reposition, so the next `next()` is the first trade at or after the open.
#[cfg(feature = "seek-to")]
fn seek_to_session_open() {
    use subms_merge_iterator::SeekableMergeIterator;
    println!("\n== seek-to: skip to the session open ==");
    let venues: Vec<std::vec::IntoIter<i64>> = vec![
        vec![8_000, 9_100, 9_400, 9_800].into_iter(),
        vec![8_500, 9_300, 9_600].into_iter(),
    ];
    let mut scan = SeekableMergeIterator::new(venues);
    let open = 9_300;
    scan.seek(&open);
    let session: Vec<i64> = scan.collect();
    println!("  first regular-session trades: {session:?}");
    assert_eq!(session.first(), Some(&9_300), "scan starts at the open");
    assert!(
        session.iter().all(|&t| t >= open),
        "no pre-market ticks leak in"
    );
}

/// `tombstones` feature: an instrument-reference read across SSTable levels. A
/// newer level can carry a tombstone (a delisting) that shadows the same symbol
/// in older levels, so the read result drops those keys entirely.
#[cfg(feature = "tombstones")]
fn tombstone_reference_read() {
    use subms_merge_iterator::{TombstoneEntry, TombstoneMergeIterator};
    println!("\n== tombstones: a delisting shadows older rows ==");
    // Sources run oldest -> newest; the higher index wins a key tie.
    let older = vec![
        TombstoneEntry::live("AAPL", "active"),
        TombstoneEntry::live("ENRN", "active"),
    ];
    let newer = vec![TombstoneEntry::tombstone("ENRN")];
    let live: Vec<_> = TombstoneMergeIterator::new([older.into_iter(), newer.into_iter()])
        .map(|e| e.key)
        .collect();
    println!("  symbols in the read result: {live:?}");
    assert_eq!(live, vec!["AAPL"], "the delisted symbol is shadowed out");
}

/// `dedup` feature: append-only price shards, each sorted by symbol, may carry
/// the same symbol more than once. Latest-source-wins collapses each symbol to
/// its freshest price - the compaction shape.
#[cfg(feature = "dedup")]
fn dedup_price_shards() {
    use subms_merge_iterator::{DedupEntry, DedupMergeIterator};
    println!("\n== dedup: freshest price per symbol ==");
    let older_shard = vec![DedupEntry::new("AAPL", 150), DedupEntry::new("MSFT", 300)];
    let newer_shard = vec![DedupEntry::new("AAPL", 152)];
    let compacted: Vec<_> =
        DedupMergeIterator::new([older_shard.into_iter(), newer_shard.into_iter()])
            .map(|e| (e.key, e.value))
            .collect();
    println!("  compacted book: {compacted:?}");
    assert_eq!(
        compacted,
        vec![("AAPL", 152), ("MSFT", 300)],
        "AAPL takes the newer price"
    );
}

/// `priority` feature: a live in-memory memtable should outrank stale on-disk
/// SSTable levels on a key tie even though it is registered first. An explicit
/// priority states that directly rather than forcing the caller to reorder.
#[cfg(feature = "priority")]
fn priority_memtable_wins() {
    use subms_merge_iterator::{PriorityEntry, PriorityMergeIterator, PrioritySource};
    println!("\n== priority: the memtable beats the SSTables ==");
    let memtable = PrioritySource::new(
        100,
        vec![PriorityEntry::new("AAPL", "live-153")].into_iter(),
    );
    let sstable = PrioritySource::new(
        10,
        vec![
            PriorityEntry::new("AAPL", "disk-150"),
            PriorityEntry::new("MSFT", "disk-300"),
        ]
        .into_iter(),
    );
    let view: Vec<_> = PriorityMergeIterator::new([memtable, sstable])
        .map(|e| (e.key, e.value))
        .collect();
    println!("  resolved read view: {view:?}");
    assert_eq!(
        view,
        vec![("AAPL", "live-153"), ("MSFT", "disk-300")],
        "the live memtable wins AAPL"
    );
}
