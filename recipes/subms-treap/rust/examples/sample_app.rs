//! Sample app: a bid-side depth book built on `subms-treap`.
//!
//! A fixed tape of order events is applied to a price-level index, then the
//! book is read the way a trading system reads it - top of book first, a band
//! around the touch, a sweep off the top - and finally rebuilt from a sorted
//! snapshot. Run the base with `cargo run --example sample_app`; add
//! `--all-features` (or a subset like `--features range-query`) to light up
//! the optional sections.
//!
//! Keys are price levels in integer ticks, values are the resting quantity at
//! that level. Everything is seeded and the tape is fixed, so the output is
//! byte-identical on every run.
//!
//! * base             - apply a tape, read the ladder, sweep the touch, restore
//! * range scan       - resting depth within a price band, ascending
//! * persistent       - version the book so a prior state stays queryable
//! * merge-split      - partition the ladder at the touch and stitch it back
//! * concurrent-reads - publish a frozen book to reader threads under writer churn

use subms_treap::Treap;

/// One line of the order tape.
enum Event {
    Post(u32, u64),
    Amend(u32, i64),
    Cancel(u32),
}

const SEED: u64 = 0xB1D;

/// A fixed tape. Deterministic input is the point: the printed report below is
/// reproducible, which a page quoting that output depends on.
const TAPE: [Event; 14] = [
    Event::Post(9998, 1_000),
    Event::Post(10_000, 500),
    Event::Post(9999, 250),
    Event::Post(10_001, 100),
    Event::Post(9997, 750),
    Event::Post(10_002, 400),
    Event::Post(9995, 300),
    Event::Post(9993, 150),
    Event::Post(9996, 600),
    Event::Amend(10_000, 150),
    Event::Amend(10_001, 800),
    Event::Cancel(9997),
    Event::Post(9994, 220),
    Event::Amend(9993, -50),
];

fn main() {
    let mut book = apply_tape();
    report(&book);
    sweep_the_touch(&mut book);
    restore_from_snapshot(&book);

    band_depth();

    #[cfg(feature = "persistent")]
    versioned_book();

    #[cfg(feature = "merge-split")]
    partition_ladder();

    #[cfg(feature = "concurrent-reads")]
    published_snapshot();
}

/// Apply the tape. A post inserts or replaces a level, an amend adjusts the
/// resting quantity in place through `get_mut` (no re-descent, no priority
/// redraw), a cancel removes the level.
fn apply_tape() -> Treap<u32, u64> {
    println!("== bid-side depth book ==");
    let book = build_book();
    println!("  applied {} events -> {} levels", TAPE.len(), book.len());
    book
}

fn build_book() -> Treap<u32, u64> {
    let mut book: Treap<u32, u64> = Treap::with_capacity(SEED, TAPE.len());
    for event in &TAPE {
        match event {
            Event::Post(px, qty) => {
                book.insert(*px, *qty);
            }
            Event::Amend(px, delta) => {
                if let Some(qty) = book.get_mut(px) {
                    *qty = qty.saturating_add_signed(*delta);
                }
            }
            Event::Cancel(px) => {
                book.remove(px);
            }
        }
    }
    assert_eq!(book.len(), 9);
    assert_eq!(
        book.get(&10_000).copied(),
        Some(650),
        "amend applied in place"
    );
    assert!(!book.contains_key(&9997), "cancelled level is gone");
    book
}

/// Read the book the way a trader does: best price first, then the touch and
/// its neighbours. `iter_rev` walks the ladder high to low; `floor` and
/// `predecessor` answer "what is at or below this price" without a scan.
fn report(book: &Treap<u32, u64>) {
    let (best_px, best_qty) = book.last().map(|(k, v)| (*k, *v)).expect("non-empty");
    println!(
        "  best bid {best_px} x {best_qty} | height {} | {} levels",
        book.height(),
        book.len()
    );

    println!("  top 5, best first:");
    for (px, qty) in book.iter_rev().take(5) {
        println!("    {px}  {qty:>5}");
    }

    let inside = book.predecessor(&best_px).map(|(k, _)| *k).unwrap();
    println!("  next level down: {inside}");
    assert_eq!(inside, 10_001);

    // A price that is not a resting level still answers, which is the whole
    // reason for an ordered index over a hash map.
    let probe = 9_990u32;
    println!(
        "  probe {probe}: floor {:?}, ceiling {:?}",
        book.floor(&probe).map(|(k, _)| *k),
        book.ceiling(&probe).map(|(k, _)| *k)
    );
    assert_eq!(book.floor(&probe), None);
    assert_eq!(book.ceiling(&probe).map(|(k, _)| *k), Some(9993));
}

/// Sweep an aggressive sell through the bid side. `pop_last` takes the best
/// level in expected O(log n) and hands back both key and value, so the fill
/// loop never re-descends to find the next price.
fn sweep_the_touch(book: &mut Treap<u32, u64>) {
    let mut to_fill = 1_200u64;
    let mut fills = Vec::new();
    while to_fill > 0 {
        let Some((px, qty)) = book.pop_last() else {
            break;
        };
        let take = qty.min(to_fill);
        to_fill -= take;
        fills.push((px, take));
        if qty > take {
            book.insert(px, qty - take); // partial fill, level survives
        }
    }
    println!("  sweep 1200 lots -> {fills:?}");
    assert_eq!(fills, vec![(10_002, 400), (10_001, 800)]);
    assert_eq!(book.len(), 8);
    assert_eq!(
        book.last().map(|(k, _)| *k),
        Some(10_001),
        "partial fill left the level"
    );
}

/// End-of-day restore. `collect_in_order` gives a sorted snapshot; `from_sorted`
/// rebuilds in O(n) instead of paying n rotating inserts.
fn restore_from_snapshot(book: &Treap<u32, u64>) {
    let snapshot: Vec<(u32, u64)> = book.iter().map(|(k, v)| (*k, *v)).collect();
    let restored = Treap::from_sorted(SEED, snapshot.clone()).expect("snapshot is sorted");
    println!(
        "  restored {} levels from a sorted snapshot, height {}",
        restored.len(),
        restored.height()
    );
    let round_tripped: Vec<(u32, u64)> = restored.iter().map(|(k, v)| (*k, *v)).collect();
    assert_eq!(round_tripped, snapshot);

    // Unsorted input is rejected rather than silently reordered.
    let bad = Treap::from_sorted(SEED, [(2u32, 1u64), (1, 1)]);
    assert!(bad.is_err(), "strictly-ascending precondition enforced");
}

/// Sum resting depth in a price band without
/// materialising the whole ladder. `range` descends to the low bound in
/// expected O(log N), then walks only the window in ascending order. Each
/// bound is independently inclusive, exclusive, or unbounded.
fn band_depth() {
    use subms_treap::RangeBound;
    println!("\n== range-query: depth in a price band ==");
    let book = build_book();
    let (lo, hi) = (9_996u32, 10_000u32);
    let band: Vec<(u32, u64)> = book
        .range(RangeBound::Inclusive(&lo), RangeBound::Inclusive(&hi))
        .map(|(k, v)| (*k, *v))
        .collect();
    let depth: u64 = band.iter().map(|(_, q)| *q).sum();
    println!("  [{lo}, {hi}] -> {} levels, {depth} lots", band.len());
    assert_eq!(
        band.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![9_996, 9_998, 9_999, 10_000]
    );
    assert_eq!(depth, 2_500);

    // Exclusive upper bound drops the touch itself.
    let inside: u64 = book
        .range(RangeBound::Inclusive(&lo), RangeBound::Exclusive(&hi))
        .map(|(_, q)| *q)
        .sum();
    println!("  same band, exclusive of {hi}: {inside} lots");
    assert_eq!(inside, 1_850);
}

/// `persistent` feature: version the book so a prior state stays queryable.
/// Each `insert` / `remove` returns a NEW book and leaves the receiver
/// untouched - the shape a risk what-if branch or an audit trail wants.
#[cfg(feature = "persistent")]
fn versioned_book() {
    use subms_treap::PersistentTreap;
    println!("\n== persistent: versioned book ==");
    let open: PersistentTreap<u32, u64> = PersistentTreap::new(SEED);
    let open = open
        .insert(9_999, 250)
        .insert(10_000, 500)
        .insert(10_001, 100);

    // Branch: what does the book look like if the 9999 level fills?
    let after_fill = open.remove(&9_999);
    println!(
        "  open: {} levels, depth@9999 {:?}",
        open.len(),
        open.get(&9_999).copied()
    );
    println!(
        "  after fill: {} levels, depth@9999 {:?}",
        after_fill.len(),
        after_fill.get(&9_999)
    );
    assert_eq!(open.get(&9_999).copied(), Some(250), "prior version intact");
    assert_eq!(after_fill.get(&9_999), None);
    assert_eq!((open.len(), after_fill.len()), (3, 2));
}

/// `merge-split` feature: partition the ladder at the touch in expected
/// O(log N), then stitch it back. This is the treap's distinguishing
/// operation - a red-black tree has no cheap equivalent. `merge` requires
/// every key on the left to be strictly less than every key on the right.
#[cfg(feature = "merge-split")]
fn partition_ladder() {
    use subms_treap::SplittableTreap;
    println!("\n== merge-split: partition at the touch ==");
    let mut book: SplittableTreap<u32, u64> = SplittableTreap::new(SEED);
    for (px, qty) in [
        (9_996u32, 600u64),
        (9_998, 1_000),
        (9_999, 250),
        (10_000, 650),
        (10_001, 900),
        (10_002, 400),
    ] {
        book.insert(px, qty);
    }

    // Everything strictly below 10000 is the resting book; 10000 and above is
    // the band a marketable order would clear against.
    let (resting, marketable) = book.split(&10_000);
    println!(
        "  below 10000: {} levels | 10000 and up: {} levels",
        resting.len(),
        marketable.len()
    );
    assert_eq!((resting.len(), marketable.len()), (3, 3));
    assert_eq!(
        marketable.collect_in_order().first().map(|(k, _)| **k),
        Some(10_000)
    );

    let rejoined = SplittableTreap::merge(resting, marketable);
    let keys: Vec<u32> = rejoined
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    println!("  rejoined: {keys:?}");
    assert_eq!(keys, vec![9_996, 9_998, 9_999, 10_000, 10_001, 10_002]);
}

/// `concurrent-reads` feature: freeze the book into a shared snapshot and fan
/// it out to reader threads (market-data / risk consumers) while the writer
/// keeps applying updates. Every reader sees a stable point-in-time book.
#[cfg(feature = "concurrent-reads")]
fn published_snapshot() {
    use std::thread;
    use subms_treap::TreapSnapshot;
    println!("\n== concurrent-reads: published book snapshot ==");
    let mut book: Treap<u32, u64> = Treap::new(SEED);
    for px in 9_990..10_010u32 {
        book.insert(px, (px as u64) * 10);
    }
    let snap = TreapSnapshot::from_treap(&book);

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let s = snap.clone();
            thread::spawn(move || s.range(&9_995, &10_004).count())
        })
        .collect();

    // Writer churn after the snapshot: readers must not observe it.
    book.insert(12_345, 1);
    book.remove(&9_990);

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
