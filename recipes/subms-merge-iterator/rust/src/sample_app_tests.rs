//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the base consolidated-tape merge stays chronological, and each feature
//! variant resolves key collisions the way the market-data framing claims.

use super::*;

#[test]
fn consolidated_tape_scenario() {
    let venues: Vec<std::vec::IntoIter<i64>> = vec![
        vec![100, 450, 780].into_iter(),
        vec![120, 460, 810].into_iter(),
        vec![90, 300, 900].into_iter(),
    ];
    let tape: Vec<i64> = MergeIterator::new(venues).collect();

    assert_eq!(tape.len(), 9, "every trade appears once");
    assert!(
        tape.windows(2).all(|w| w[0] <= w[1]),
        "tape stays chronological"
    );
    assert_eq!(tape.first(), Some(&90), "earliest trade leads");
    assert_eq!(tape.last(), Some(&900), "latest trade trails");
}

#[cfg(feature = "seek-to")]
#[test]
fn seek_skips_pre_market() {
    use crate::SeekableMergeIterator;
    let venues: Vec<std::vec::IntoIter<i64>> = vec![
        vec![8_000, 9_100, 9_400, 9_800].into_iter(),
        vec![8_500, 9_300, 9_600].into_iter(),
    ];
    let mut scan = SeekableMergeIterator::new(venues);
    scan.seek(&9_300);
    scan.set_upper_bound(9_800);
    let session: Vec<i64> = scan.collect();
    assert_eq!(
        session,
        vec![9_300, 9_400, 9_600],
        "scan starts at the open and stops before the close"
    );
}

#[cfg(feature = "reverse")]
#[test]
fn reverse_walks_the_bid_ladder_down() {
    use crate::ReverseMergeIterator;
    let ladders: Vec<std::vec::IntoIter<i64>> = vec![
        vec![10_120, 10_105, 10_101, 10_095].into_iter(),
        vec![10_118, 10_110, 10_099].into_iter(),
    ];
    let mut book = ReverseMergeIterator::new(ladders);
    assert_eq!(book.peek(), Some(&10_120), "best bid leads the walk");
    book.seek_for_prev(&10_110);
    book.set_lower_bound(10_100);
    let band: Vec<i64> = book.collect();
    assert_eq!(band, vec![10_110, 10_105, 10_101]);
}

#[cfg(feature = "tombstones")]
#[test]
fn tombstone_shadows_delisted_symbol() {
    use crate::{TombstoneEntry, TombstoneMergeIterator};
    let older = vec![
        TombstoneEntry::live("AAPL", "active"),
        TombstoneEntry::live("ENRN", "active"),
    ];
    let newer = vec![TombstoneEntry::tombstone("ENRN")];
    let live: Vec<_> = TombstoneMergeIterator::new([older.into_iter(), newer.into_iter()])
        .map(|e| e.key)
        .collect();
    assert_eq!(live, vec!["AAPL"], "the delisted symbol is dropped");
}

#[cfg(feature = "dedup")]
#[test]
fn dedup_keeps_freshest_price() {
    use crate::{DedupEntry, DedupMergeIterator};
    let older_shard = vec![DedupEntry::new("AAPL", 150), DedupEntry::new("MSFT", 300)];
    let newer_shard = vec![DedupEntry::new("AAPL", 152)];
    let compacted: Vec<_> =
        DedupMergeIterator::new([older_shard.into_iter(), newer_shard.into_iter()])
            .map(|e| (e.key, e.value))
            .collect();
    assert_eq!(compacted, vec![("AAPL", 152), ("MSFT", 300)]);
}

#[cfg(feature = "priority")]
#[test]
fn priority_memtable_outranks_disk() {
    use crate::{PriorityEntry, PriorityMergeIterator, PrioritySource};
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
    assert_eq!(view, vec![("AAPL", "live-153"), ("MSFT", "disk-300")]);
}
