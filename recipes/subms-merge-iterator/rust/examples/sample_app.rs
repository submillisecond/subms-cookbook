//! Sample app: a miniature market-data store read entirely through merge
//! iterators. One session of data is declared up front; every section below is
//! a different query against it.
//!
//! Run the base with `cargo run --example sample_app`; add `--features full`
//! to light up the opt-in sections.
//!
//! The store holds three things, the way an LSM-backed tick store does:
//! a trade tape per venue, a bid ladder per venue, and reference and last-price
//! rows spread across levels (oldest flushed level first, live memtable last).
//!
//! * base       - consolidate the per-venue tapes into one chronological tape
//! * seek-to    - read one half-open session window out of that tape
//! * reverse    - walk the consolidated bid ladder down from the top of book
//! * tombstones - resolve the reference rows, honouring a delisting
//! * dedup      - collapse the last-price rows to the freshest per symbol
//! * priority   - the same rows, with the memtable stated as authoritative

use subms_merge_iterator::MergeIterator;

/// Trade timestamps in ns since the session epoch, one ascending tape per
/// venue. 9_300 is the open and 9_800 the close.
const VENUE_TAPES: [&[i64]; 3] = [
    &[8_000, 9_100, 9_400, 9_800],
    &[8_500, 9_300, 9_600],
    &[9_050, 9_450, 9_900],
];

/// Bid price levels in ticks, one descending ladder per venue - the order a
/// depth feed already publishes them in.
const BID_LADDERS: [&[i64]; 2] = [&[10_120, 10_105, 10_101, 10_095], &[10_118, 10_110, 10_099]];

/// Instrument reference rows, oldest level first, sorted by symbol within a
/// level. `None` is a tombstone: the delisting written to the newest level.
const REFERENCE_LEVELS: [&[(&str, Option<&str>)]; 3] = [
    &[
        ("AAPL", Some("listed")),
        ("ENRN", Some("listed")),
        ("MSFT", Some("listed")),
    ],
    &[("AAPL", Some("listed-adr"))],
    &[("ENRN", None)],
];

/// Last-price rows. The flushed level is stale for AAPL; the memtable holds the
/// write that has not reached disk yet.
#[cfg(any(feature = "dedup", feature = "priority"))]
const PRICE_FLUSHED: &[(&str, i64)] = &[("AAPL", 150), ("MSFT", 300)];
#[cfg(any(feature = "dedup", feature = "priority"))]
const PRICE_MEMTABLE: &[(&str, i64)] = &[("AAPL", 152)];

fn main() {
    println!(
        "market-data store: {} venue tapes, {} bid ladders, {} reference levels",
        VENUE_TAPES.len(),
        BID_LADDERS.len(),
        REFERENCE_LEVELS.len()
    );

    base_consolidated_tape();

    #[cfg(feature = "seek-to")]
    session_window_scan();

    #[cfg(feature = "reverse")]
    walk_bid_ladder_down();

    #[cfg(feature = "tombstones")]
    resolve_reference_rows();

    #[cfg(feature = "dedup")]
    compact_last_prices();

    #[cfg(feature = "priority")]
    memtable_wins_the_read();
}

fn tapes() -> Vec<std::iter::Copied<std::slice::Iter<'static, i64>>> {
    VENUE_TAPES.iter().map(|t| t.iter().copied()).collect()
}

/// Base API: each venue publishes trades already sorted by exchange timestamp.
/// Merging their heads on a min-heap gives one chronological consolidated tape
/// without materialising and re-sorting the union.
fn base_consolidated_tape() {
    println!("\n== base: consolidated trade tape ==");
    let merge = MergeIterator::new(tapes());
    println!("  live venues: {}", merge.live_streams());
    println!("  earliest trade: {:?}", merge.peek());
    let tape: Vec<i64> = merge.collect();
    println!("  {} trades in order: {tape:?}", tape.len());
    assert_eq!(tape.len(), 10, "every trade appears once");
    assert!(
        tape.windows(2).all(|w| w[0] <= w[1]),
        "the tape stays chronological"
    );
}

/// `seek-to`: a regular-session query wants `[open, close)` and nothing else.
/// `seek(open)` advances every venue past its pre-market ticks in one bounded
/// reposition; `set_upper_bound(close)` ends the scan, so the caller pulls
/// `next()` until it stops rather than testing each element itself.
#[cfg(feature = "seek-to")]
fn session_window_scan() {
    use subms_merge_iterator::SeekableMergeIterator;
    println!("\n== seek-to: one session window out of the tape ==");
    let (open, close) = (9_300, 9_800);

    let mut scan = SeekableMergeIterator::new(tapes());
    scan.seek(&open);
    scan.set_upper_bound(close);

    let window: Vec<i64> = scan.collect();
    println!("  window [{open}, {close}): {window:?}");
    assert_eq!(
        window,
        vec![9_300, 9_400, 9_450, 9_600],
        "half-open: the close tick is excluded"
    );
}

/// `reverse`: a bid ladder is quoted best-price-first, so it arrives sorted
/// descending already. Merging the ladders descending gives one consolidated
/// book. Pricing a marketable sell only needs the levels between the touch and
/// a limit, so `seek_for_prev` starts the walk and `set_lower_bound` ends it -
/// the rest of the book is never read.
#[cfg(feature = "reverse")]
fn walk_bid_ladder_down() {
    use subms_merge_iterator::ReverseMergeIterator;
    println!("\n== reverse: walk the consolidated bid ladder down ==");
    let ladders: Vec<_> = BID_LADDERS.iter().map(|l| l.iter().copied()).collect();

    let mut book = ReverseMergeIterator::new(ladders);
    println!("  best bid across venues: {:?}", book.peek());

    let limit = 10_100;
    book.seek_for_prev(&10_110);
    book.set_lower_bound(limit);

    let fillable: Vec<i64> = book.collect();
    println!("  levels from 10110 down to the {limit} limit: {fillable:?}");
    assert_eq!(
        fillable,
        vec![10_110, 10_105, 10_101],
        "descending, and the lower bound is inclusive"
    );
}

/// `tombstones`: a reference read across three levels. The newest level's
/// delisting shadows the same symbol everywhere below it, so the key leaves the
/// result entirely; AAPL takes its newer status from the middle level.
#[cfg(feature = "tombstones")]
fn resolve_reference_rows() {
    use subms_merge_iterator::{TombstoneEntry, TombstoneMergeIterator};
    println!("\n== tombstones: resolve the reference rows ==");
    let levels: Vec<std::vec::IntoIter<TombstoneEntry<&str, &str>>> = REFERENCE_LEVELS
        .iter()
        .map(|rows| {
            rows.iter()
                .map(|&(sym, status)| match status {
                    Some(s) => TombstoneEntry::live(sym, s),
                    None => TombstoneEntry::tombstone(sym),
                })
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();

    let resolved: Vec<(&str, &str)> = TombstoneMergeIterator::new(levels)
        .map(|e| (e.key, e.value.unwrap()))
        .collect();
    println!("  live instruments: {resolved:?}");
    assert_eq!(
        resolved,
        vec![("AAPL", "listed-adr"), ("MSFT", "listed")],
        "the delisted symbol is shadowed out, AAPL takes the newer row"
    );
}

/// `dedup`: the same symbol appears in the flushed level and the memtable.
/// Latest-source-wins collapses each symbol to one row, which is the compaction
/// output. Registration order carries the recency here.
#[cfg(feature = "dedup")]
fn compact_last_prices() {
    use subms_merge_iterator::{DedupEntry, DedupMergeIterator};
    println!("\n== dedup: compact the last-price rows ==");
    let flushed = PRICE_FLUSHED
        .iter()
        .map(|&(k, v)| DedupEntry::new(k, v))
        .collect::<Vec<_>>();
    let memtable = PRICE_MEMTABLE
        .iter()
        .map(|&(k, v)| DedupEntry::new(k, v))
        .collect::<Vec<_>>();

    let compacted: Vec<(&str, i64)> =
        DedupMergeIterator::new([flushed.into_iter(), memtable.into_iter()])
            .map(|e| (e.key, e.value))
            .collect();
    println!("  compacted last prices: {compacted:?}");
    assert_eq!(compacted, vec![("AAPL", 152), ("MSFT", 300)]);
}

/// `priority`: the same two sources, registered the other way round - a read
/// path holds the memtable first. Registration order now says the wrong thing,
/// so authority is stated explicitly and the merge still resolves AAPL to the
/// unflushed write.
#[cfg(feature = "priority")]
fn memtable_wins_the_read() {
    use subms_merge_iterator::{PriorityEntry, PriorityMergeIterator, PrioritySource};
    println!("\n== priority: the memtable is authoritative ==");
    let memtable = PrioritySource::new(
        100,
        PRICE_MEMTABLE
            .iter()
            .map(|&(k, v)| PriorityEntry::new(k, v))
            .collect::<Vec<_>>()
            .into_iter(),
    );
    let flushed = PrioritySource::new(
        10,
        PRICE_FLUSHED
            .iter()
            .map(|&(k, v)| PriorityEntry::new(k, v))
            .collect::<Vec<_>>()
            .into_iter(),
    );

    let view: Vec<(&str, i64)> = PriorityMergeIterator::new([memtable, flushed])
        .map(|e| (e.key, e.value))
        .collect();
    println!("  resolved read view: {view:?}");
    assert_eq!(
        view,
        vec![("AAPL", 152), ("MSFT", 300)],
        "the memtable wins AAPL despite being registered first"
    );
}
