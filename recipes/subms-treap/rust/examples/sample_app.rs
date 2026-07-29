//! Sample app: a tour of `subms-treap` as a limit-order-book price-level
//! index, base API first, then each optional feature. Run the base with
//! `cargo run --example sample_app`; add `--all-features` (or a subset like
//! `--features range-query`) to light up the feature sections.
//!
//! Keys are price levels in integer ticks, values are the resting quantity
//! at that level. The ordered map keeps the ladder sorted by price, so a
//! reader can walk it, query a band, snapshot it, or split it at a price.
//!
//! * base             - a bid ladder: post, amend, read, cancel a level
//! * range-query      - resting depth within a price band, ascending
//! * persistent       - snapshot the book, keep prior versions queryable
//! * merge-split      - partition the ladder at a price and stitch it back
//! * concurrent-reads - publish a frozen book to reader threads under writer churn

use subms_treap::Treap;

fn main() {
    base_bid_ladder();

    #[cfg(feature = "range-query")]
    band_depth();

    #[cfg(feature = "persistent")]
    versioned_book();

    #[cfg(feature = "merge-split")]
    partition_ladder();

    #[cfg(feature = "concurrent-reads")]
    published_snapshot();
}

/// Base API: a single-sided bid ladder keyed by price tick, valued by the
/// resting quantity at that level. `insert` posts or amends a level, `get`
/// reads depth, `remove` cancels a level, and `collect_in_order` walks the
/// ladder low price to high.
fn base_bid_ladder() {
    println!("== base: bid ladder ==");
    let mut book: Treap<u32, u64> = Treap::new(0xB1D);
    for (px, qty) in [
        (9998u32, 1_000u64),
        (10_000, 500),
        (9999, 250),
        (10_001, 100),
        (9997, 750),
    ] {
        book.insert(px, qty);
    }
    println!("  posted {} price levels", book.len());
    assert_eq!(book.len(), 5);

    // A fresh post at an existing level replaces the resting quantity.
    let prev = book.insert(10_000, 650);
    println!(
        "  amend 10000: was {:?}, now {:?}",
        prev,
        book.get(&10_000).copied()
    );
    assert_eq!(prev, Some(500));
    assert_eq!(book.get(&10_000).copied(), Some(650));

    let cancelled = book.remove(&9997);
    println!("  cancel 9997 -> {cancelled:?}");
    assert_eq!(cancelled, Some(750));
    assert_eq!(book.get(&9997), None);

    let ladder: Vec<(u32, u64)> = book
        .collect_in_order()
        .into_iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    println!("  ladder (low->high): {ladder:?}");
    assert_eq!(
        ladder,
        vec![(9998, 1_000), (9999, 250), (10_000, 650), (10_001, 100)]
    );
    for w in ladder.windows(2) {
        assert!(w[0].0 < w[1].0, "ladder stays sorted by price");
    }
}

/// `range-query` feature: sum resting depth in a price band without
/// materialising the whole ladder. `range` descends to the low bound in
/// expected O(log N), then walks only the window in ascending order. Each
/// bound is independently inclusive, exclusive, or unbounded.
#[cfg(feature = "range-query")]
fn band_depth() {
    use subms_treap::RangeBound;
    println!("\n== range-query: depth in a price band ==");
    let mut book: Treap<u32, u64> = Treap::new(0xB1D);
    for (px, qty) in [
        (9998u32, 1_000u64),
        (9999, 250),
        (10_000, 650),
        (10_001, 100),
        (10_002, 900),
    ] {
        book.insert(px, qty);
    }
    let (lo, hi) = (9999u32, 10_001u32);
    let band: Vec<(u32, u64)> = book
        .range(RangeBound::Inclusive(&lo), RangeBound::Inclusive(&hi))
        .map(|(k, v)| (*k, *v))
        .collect();
    let depth: u64 = band.iter().map(|(_, q)| *q).sum();
    println!("  [{lo}, {hi}] -> {band:?}, total depth {depth}");
    assert_eq!(
        band.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![9999, 10_000, 10_001]
    );
    assert_eq!(depth, 1_000);
}

/// `persistent` feature: snapshot the book cheaply and keep prior versions
/// queryable. Each `insert` / `remove` returns a NEW book and leaves the
/// receiver untouched - the shape an audit trail or a what-if branch wants.
#[cfg(feature = "persistent")]
fn versioned_book() {
    use subms_treap::PersistentTreap;
    println!("\n== persistent: versioned book ==");
    let v0: PersistentTreap<u32, u64> = PersistentTreap::new(0xB1D);
    let v1 = v0.insert(10_000, 500).insert(9999, 250);
    let v2 = v1.remove(&9999); // 9999 fully filled
    println!(
        "  v1 depth@9999 {:?}, v2 depth@9999 {:?}",
        v1.get(&9999).copied(),
        v2.get(&9999)
    );
    assert_eq!(
        v1.get(&9999).copied(),
        Some(250),
        "old version still holds the filled level"
    );
    assert_eq!(v2.get(&9999), None);
    assert_eq!(v1.len(), 2);
    assert_eq!(v2.len(), 1);
}

/// `merge-split` feature: partition the ladder at a price in expected
/// O(log N), then stitch it back. `split(pivot)` puts every level below
/// `pivot` in `below` and `pivot`-and-above in `at_and_above`; `merge`
/// requires every key on the left to be strictly less than every key on
/// the right.
#[cfg(feature = "merge-split")]
fn partition_ladder() {
    use subms_treap::SplittableTreap;
    println!("\n== merge-split: partition the ladder ==");
    let mut book: SplittableTreap<u32, u64> = SplittableTreap::new(0xB1D);
    for (px, qty) in [
        (9998u32, 1_000u64),
        (9999, 250),
        (10_000, 650),
        (10_001, 100),
    ] {
        book.insert(px, qty);
    }
    let (below, at_and_above) = book.split(&10_000);
    println!(
        "  below 10000: {} levels, 10000+: {} levels",
        below.len(),
        at_and_above.len()
    );
    assert_eq!(below.len(), 2); // 9998, 9999
    assert_eq!(at_and_above.len(), 2); // 10000, 10001
    let rejoined = SplittableTreap::merge(below, at_and_above);
    let keys: Vec<u32> = rejoined
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    println!("  rejoined: {keys:?}");
    assert_eq!(keys, vec![9998, 9999, 10_000, 10_001]);
}

/// `concurrent-reads` feature: freeze the book into a shared snapshot and fan
/// it out to reader threads (market-data / risk consumers) while the writer
/// keeps applying updates. Every reader sees a stable point-in-time book.
#[cfg(feature = "concurrent-reads")]
fn published_snapshot() {
    use std::thread;
    use subms_treap::TreapSnapshot;
    println!("\n== concurrent-reads: published book snapshot ==");
    let mut book: Treap<u32, u64> = Treap::new(0xB1D);
    for px in 9990..10_010u32 {
        book.insert(px, (px as u64) * 10);
    }
    let snap = TreapSnapshot::from_treap(&book);

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let s = snap.clone();
            thread::spawn(move || s.range(&9995, &10_004).count())
        })
        .collect();

    // Writer churn after the snapshot: readers must not observe it.
    book.insert(12_345, 1);
    book.remove(&9990);

    for r in readers {
        assert_eq!(
            r.join().unwrap(),
            10,
            "reader sees the frozen 10-level band"
        );
    }
    println!("  4 readers each counted 10 levels in [9995, 10004]");
    assert!(
        snap.get(&12_345).is_none(),
        "snapshot isolated from later writes"
    );
    assert_eq!(snap.len(), 20);
}
