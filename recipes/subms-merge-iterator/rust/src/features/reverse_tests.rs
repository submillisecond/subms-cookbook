use super::*;

/// Descending sources - the direction this iterator contracts for.
fn streams<const N: usize>(data: [Vec<i32>; N]) -> Vec<std::vec::IntoIter<i32>> {
    data.into_iter().map(|v| v.into_iter()).collect()
}

#[test]
fn merges_descending_streams() {
    let it = ReverseMergeIterator::new(streams([vec![7, 4, 1], vec![8, 5, 2], vec![9, 6, 3]]));
    let merged: Vec<_> = it.collect();
    assert_eq!(merged, (1..=9).rev().collect::<Vec<_>>());
}

#[test]
fn fresh_iterator_sits_on_the_largest_value() {
    let it = ReverseMergeIterator::new(streams([vec![40, 10], vec![90, 20]]));
    assert_eq!(it.peek(), Some(&90));
    assert_eq!(it.live_streams(), 2);
    assert_eq!(it.num_streams(), 2);
}

#[test]
fn handles_empty_and_absent_streams() {
    let it = ReverseMergeIterator::new(streams([vec![], vec![5, 1], vec![]]));
    assert_eq!(it.num_streams(), 3);
    assert_eq!(it.live_streams(), 1);
    assert_eq!(it.collect::<Vec<_>>(), vec![5, 1]);

    let empty = ReverseMergeIterator::new(streams::<0>([]));
    assert_eq!(empty.peek(), None);
    assert_eq!(empty.collect::<Vec<_>>(), Vec::<i32>::new());
}

#[test]
fn duplicates_across_streams_all_appear() {
    let it = ReverseMergeIterator::new(streams([vec![3, 2, 1], vec![4, 3, 2]]));
    assert_eq!(it.collect::<Vec<_>>(), vec![4, 3, 3, 2, 2, 1]);
}

#[test]
fn seek_for_prev_lands_on_largest_le_target() {
    let mut it = ReverseMergeIterator::new(streams([
        vec![10, 7, 4, 1],
        vec![11, 8, 5, 2],
        vec![12, 9, 6, 3],
    ]));
    it.seek_for_prev(&6);
    assert_eq!(it.collect::<Vec<_>>(), vec![6, 5, 4, 3, 2, 1]);
}

#[test]
fn seek_for_prev_with_target_above_max_is_noop() {
    let mut it = ReverseMergeIterator::new(streams([vec![7, 6, 5], vec![10, 9, 8]]));
    it.seek_for_prev(&100);
    assert_eq!(it.collect::<Vec<_>>(), vec![10, 9, 8, 7, 6, 5]);
}

#[test]
fn seek_for_prev_below_every_value_exhausts() {
    let mut it = ReverseMergeIterator::new(streams([vec![3, 2, 1], vec![6, 5, 4]]));
    it.seek_for_prev(&0);
    assert_eq!(it.next(), None);
    assert_eq!(it.next(), None);
}

#[test]
fn seek_for_prev_on_empty_sources_is_safe() {
    let mut it = ReverseMergeIterator::new(streams([vec![], vec![], vec![]]));
    it.seek_for_prev(&42);
    assert_eq!(it.next(), None);
}

#[test]
fn repeated_seek_for_prev_is_monotonic() {
    let mut it = ReverseMergeIterator::new(streams([
        vec![10, 9, 8, 7, 6, 5, 4, 3, 2, 1],
        vec![17, 16, 15],
    ]));
    it.seek_for_prev(&16);
    assert_eq!(it.next(), Some(16));
    it.seek_for_prev(&9);
    assert_eq!(it.next(), Some(9));
    it.seek_for_prev(&6);
    assert_eq!(it.next(), Some(6));
    it.seek_for_prev(&0);
    assert_eq!(it.next(), None);
}

#[test]
fn seek_for_prev_only_walks_streams_above_target() {
    // One head already <= target takes the early-stop arm; the other two
    // walk forward. Both arms run inside one call.
    let mut it = ReverseMergeIterator::new(streams([vec![50, 2, 1], vec![31, 30], vec![40, 5]]));
    it.seek_for_prev(&31);
    assert_eq!(it.collect::<Vec<_>>(), vec![31, 30, 5, 2, 1]);
}

#[test]
fn lower_bound_is_inclusive_and_stops_the_scan() {
    let mut it = ReverseMergeIterator::new(streams([vec![9, 6, 3], vec![8, 5, 2]]));
    it.set_lower_bound(5);
    assert_eq!(it.collect::<Vec<_>>(), vec![9, 8, 6, 5]);
}

#[test]
fn lower_bound_can_be_cleared_and_the_rest_still_reads() {
    let mut it = ReverseMergeIterator::new(streams([vec![9, 6, 3], vec![8, 5, 2]]));
    it.set_lower_bound(6);
    assert_eq!(it.next(), Some(9));
    assert_eq!(it.next(), Some(8));
    assert_eq!(it.next(), Some(6));
    assert_eq!(it.peek(), None);
    assert_eq!(it.next(), None);
    it.clear_lower_bound();
    assert_eq!(it.collect::<Vec<_>>(), vec![5, 3, 2]);
}

#[test]
fn lower_bound_above_the_head_exhausts_immediately() {
    let mut it = ReverseMergeIterator::new(streams([vec![4, 3], vec![2, 1]]));
    it.set_lower_bound(99);
    assert_eq!(it.peek(), None);
    assert_eq!(it.next(), None);
}

#[test]
fn bounded_window_scan_reads_a_price_band() {
    // seek_for_prev(hi) + set_lower_bound(lo) is the descending [lo, hi]
    // window, the shape a book walk from the top of book downward wants.
    let mut it = ReverseMergeIterator::new(streams([vec![120, 105, 101, 95], vec![118, 110, 99]]));
    it.seek_for_prev(&110);
    it.set_lower_bound(100);
    assert_eq!(it.collect::<Vec<_>>(), vec![110, 105, 101]);
}

#[test]
fn descending_merge_holds_over_ten_streams() {
    let streams: Vec<std::vec::IntoIter<i32>> = (0..10)
        .map(|s| {
            (0..1000)
                .map(move |i| s + 10 * (999 - i))
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();
    let merged: Vec<_> = ReverseMergeIterator::new(streams).collect();
    assert_eq!(merged.len(), 10_000);
    for w in merged.windows(2) {
        assert!(w[0] >= w[1], "descending");
    }
}
