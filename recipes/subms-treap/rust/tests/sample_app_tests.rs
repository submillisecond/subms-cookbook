//! Pins the behaviour each section of the `sample_app` example demonstrates:
//! the bid-ladder base scenario and each optional feature section, gated the
//! same way as the sample. Std-only, not harness-gated.

use subms_treap::Treap;

#[test]
fn bid_ladder_scenario() {
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
    assert_eq!(book.len(), 5);

    assert_eq!(
        book.insert(10_000, 650),
        Some(500),
        "amend replaces resting quantity"
    );
    assert_eq!(book.get(&10_000).copied(), Some(650));

    assert_eq!(
        book.remove(&9997),
        Some(750),
        "cancel returns the removed quantity"
    );
    assert_eq!(book.get(&9997), None);

    let ladder: Vec<(u32, u64)> = book
        .collect_in_order()
        .into_iter()
        .map(|(k, v)| (*k, *v))
        .collect();
    assert_eq!(
        ladder,
        vec![(9998, 1_000), (9999, 250), (10_000, 650), (10_001, 100)]
    );
    for w in ladder.windows(2) {
        assert!(w[0].0 < w[1].0, "ladder stays sorted by price");
    }
}

#[cfg(feature = "range-query")]
#[test]
fn band_depth_is_windowed_and_sorted() {
    use subms_treap::RangeBound;
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
    assert_eq!(
        band.iter().map(|(k, _)| *k).collect::<Vec<_>>(),
        vec![9999, 10_000, 10_001]
    );
    assert_eq!(band.iter().map(|(_, q)| *q).sum::<u64>(), 1_000);
}

#[cfg(feature = "persistent")]
#[test]
fn versioned_book_keeps_prior_state() {
    use subms_treap::PersistentTreap;
    let v0: PersistentTreap<u32, u64> = PersistentTreap::new(0xB1D);
    let v1 = v0.insert(10_000, 500).insert(9999, 250);
    let v2 = v1.remove(&9999);
    assert_eq!(
        v1.get(&9999).copied(),
        Some(250),
        "old version untouched by later fill"
    );
    assert_eq!(v2.get(&9999), None);
    assert_eq!(v1.len(), 2);
    assert_eq!(v2.len(), 1);
}

#[cfg(feature = "merge-split")]
#[test]
fn split_then_merge_round_trips() {
    use subms_treap::SplittableTreap;
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
    assert_eq!(below.len(), 2);
    assert_eq!(at_and_above.len(), 2);
    let rejoined = SplittableTreap::merge(below, at_and_above);
    let keys: Vec<u32> = rejoined
        .collect_in_order()
        .into_iter()
        .map(|(k, _)| *k)
        .collect();
    assert_eq!(keys, vec![9998, 9999, 10_000, 10_001]);
}

#[cfg(feature = "concurrent-reads")]
#[test]
fn published_snapshot_isolated_from_writes() {
    use subms_treap::TreapSnapshot;
    let mut book: Treap<u32, u64> = Treap::new(0xB1D);
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
