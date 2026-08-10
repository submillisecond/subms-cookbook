//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the depth-book scenario built from the example's fixed tape, and each
//! optional feature section, gated the same way as the sample. An example
//! target cannot be imported from a lib test, so the tape is mirrored here -
//! any drift shows up as a failure on the numbers the page quotes. Std-only,
//! not harness-gated. Colocated with `lib.rs` and included via `#[path]`.

use super::*;

const SEED: u64 = 0xB1D;

enum Event {
    Post(u32, u64),
    Amend(u32, i64),
    Cancel(u32),
}

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
    book
}

#[test]
fn tape_produces_the_documented_book() {
    let book = build_book();
    assert_eq!(book.len(), 9);
    assert_eq!(book.get(&10_000).copied(), Some(650), "two amends applied");
    assert_eq!(
        book.get(&9993).copied(),
        Some(100),
        "negative amend applied"
    );
    assert!(!book.contains_key(&9997), "cancelled level is gone");
    assert_eq!(book.first().map(|(k, _)| *k), Some(9993));
    assert_eq!(book.last().map(|(k, _)| *k), Some(10_002));
}

#[test]
fn report_reads_the_book_top_down() {
    let book = build_book();
    let top: Vec<(u32, u64)> = book.iter_rev().take(5).map(|(k, v)| (*k, *v)).collect();
    assert_eq!(
        top,
        vec![
            (10_002, 400),
            (10_001, 900),
            (10_000, 650),
            (9999, 250),
            (9998, 1_000)
        ]
    );
    assert_eq!(book.predecessor(&10_002).map(|(k, _)| *k), Some(10_001));
    assert_eq!(book.floor(&9_990), None, "nothing rests below the probe");
    assert_eq!(book.ceiling(&9_990).map(|(k, _)| *k), Some(9993));
}

#[test]
fn sweep_takes_the_touch_and_leaves_a_partial() {
    let mut book = build_book();
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
            book.insert(px, qty - take);
        }
    }
    assert_eq!(fills, vec![(10_002, 400), (10_001, 800)]);
    assert_eq!(book.len(), 8);
    assert_eq!(book.get(&10_001).copied(), Some(100), "partial fill rests");
}

#[test]
fn restore_rebuilds_from_a_sorted_snapshot() {
    let book = build_book();
    let snapshot: Vec<(u32, u64)> = book.iter().map(|(k, v)| (*k, *v)).collect();
    let restored = Treap::from_sorted(SEED, snapshot.clone()).expect("sorted");
    assert_eq!(
        restored.iter().map(|(k, v)| (*k, *v)).collect::<Vec<_>>(),
        snapshot
    );
    assert!(
        restored.height() <= book.height() + 4,
        "bulk build stays shallow"
    );
    assert!(Treap::from_sorted(SEED, [(2u32, 1u64), (1, 1)]).is_err());
}

#[test]
fn band_depth_is_windowed_and_sorted() {
    let book = build_book();
    let (lo, hi) = (9_996u32, 10_000u32);
    let band: Vec<(u32, u64)> = book
        .range(RangeBound::Inclusive(&lo), RangeBound::Inclusive(&hi))
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(
        band.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![9_996, 9_998, 9_999, 10_000]
    );
    assert_eq!(band.iter().map(|(_, q)| *q).sum::<u64>(), 2_500);

    let inside: u64 = book
        .range(RangeBound::Inclusive(&lo), RangeBound::Exclusive(&hi))
        .map(|(_, q)| *q)
        .sum();
    assert_eq!(inside, 1_850, "exclusive upper bound drops the touch");
}

#[cfg(feature = "persistent")]
#[test]
fn versioned_book_keeps_prior_state() {
    let open: PersistentTreap<u32, u64> = PersistentTreap::new(SEED);
    let open = open
        .insert(9_999, 250)
        .insert(10_000, 500)
        .insert(10_001, 100);
    let after_fill = open.remove(&9_999);
    assert_eq!(
        open.get(&9_999).copied(),
        Some(250),
        "old version untouched by later fill"
    );
    assert_eq!(after_fill.get(&9_999), None);
    assert_eq!((open.len(), after_fill.len()), (3, 2));
}

#[cfg(feature = "merge-split")]
#[test]
fn split_then_merge_round_trips() {
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
    let (resting, marketable) = book.split(&10_000);
    assert_eq!((resting.len(), marketable.len()), (3, 3));
    assert_eq!(
        marketable.collect_in_order().first().map(|(k, _)| **k),
        Some(10_000),
        "pivot lands on the right"
    );

    let rejoined = SplittableTreap::merge(resting, marketable);
    let keys: Vec<u32> = rejoined
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(keys, vec![9_996, 9_998, 9_999, 10_000, 10_001, 10_002]);
}

#[cfg(feature = "concurrent-reads")]
#[test]
fn published_snapshot_isolated_from_writes() {
    let mut book: Treap<u32, u64> = Treap::new(SEED);
    for px in 9990..10_010u32 {
        book.insert(px, (px as u64) * 10);
    }
    let snap = TreapSnapshot::from_treap(&book);

    book.insert(12_345, 1);
    book.remove(&9990);

    assert_eq!(snap.range(&9995, &10_004).count(), 10, "frozen band count");
    assert!(
        snap.get(&12_345).is_none(),
        "snapshot isolated from later writes"
    );
    assert_eq!(
        snap.get(&9990).copied(),
        Some(99_900),
        "removed key still in snapshot"
    );
    assert_eq!(snap.len(), 20);
}
