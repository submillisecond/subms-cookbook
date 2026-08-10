use super::*;

fn streams<const N: usize>(data: [Vec<i32>; N]) -> Vec<std::vec::IntoIter<i32>> {
    data.into_iter().map(|v| v.into_iter()).collect()
}

#[test]
fn seek_lands_on_smallest_ge_target() {
    let mut it = SeekableMergeIterator::new(streams([
        vec![1, 4, 7, 10],
        vec![2, 5, 8, 11],
        vec![3, 6, 9, 12],
    ]));
    it.seek(&6);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![6, 7, 8, 9, 10, 11, 12]);
}

#[test]
fn seek_with_target_below_min_is_noop() {
    let mut it = SeekableMergeIterator::new(streams([vec![5, 6, 7], vec![8, 9, 10]]));
    it.seek(&0);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![5, 6, 7, 8, 9, 10]);
}

#[test]
fn seek_past_end_exhausts_iterator() {
    let mut it = SeekableMergeIterator::new(streams([vec![1, 2, 3], vec![4, 5, 6]]));
    it.seek(&1000);
    assert_eq!(it.next(), None);
    assert_eq!(it.next(), None);
}

#[test]
fn seek_on_empty_sources_is_safe() {
    let mut it = SeekableMergeIterator::new(streams([vec![], vec![], vec![]]));
    it.seek(&42);
    assert_eq!(it.next(), None);
}

#[test]
fn repeated_seeks_are_monotonic() {
    let mut it = SeekableMergeIterator::new(streams([
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        vec![15, 16, 17],
    ]));
    it.seek(&5);
    assert_eq!(it.next(), Some(5));
    it.seek(&8);
    assert_eq!(it.next(), Some(8));
    it.seek(&14);
    assert_eq!(it.next(), Some(15));
    it.seek(&20);
    assert_eq!(it.next(), None);
}

#[test]
fn seek_target_exactly_present_yields_it() {
    let mut it =
        SeekableMergeIterator::new(streams([vec![1, 5, 9], vec![2, 6, 10], vec![3, 7, 11]]));
    it.seek(&7);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![7, 9, 10, 11]);
}

#[test]
fn seek_walks_each_stream_forward_to_find_ge_target() {
    // Every stream head starts strictly below the target, so seek must
    // walk each stream forward until it lands on a value >= target
    // (the inner for-loop's found+break arm), for multiple streams.
    let mut it = SeekableMergeIterator::new(streams([
        vec![1, 2, 3, 20, 21],
        vec![4, 5, 6, 22, 23],
        vec![7, 8, 9, 24, 25],
    ]));
    it.seek(&20);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![20, 21, 22, 23, 24, 25]);
}

#[test]
fn seek_when_head_already_ge_target_stops_immediately() {
    // Min head (5) is already >= target (3): the peek branch takes the
    // else arm and breaks without walking any stream.
    let mut it = SeekableMergeIterator::new(streams([vec![5, 6, 7], vec![9, 10]]));
    it.seek(&3);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![5, 6, 7, 9, 10]);
}

#[test]
fn seek_partially_walks_only_the_streams_below_target() {
    // Some heads below, some at/above target: exercises both the walk
    // arm and the early-stop else arm in the same seek call.
    let mut it = SeekableMergeIterator::new(streams([vec![1, 2, 50], vec![30, 31], vec![5, 40]]));
    it.seek(&30);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![30, 31, 40, 50]);
}

#[test]
fn seek_with_some_exhausted_streams() {
    // Stream 0 ends well before seek target.
    let mut it = SeekableMergeIterator::new(streams([vec![1, 2, 3], vec![10, 20, 30]]));
    it.seek(&15);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![20, 30]);
}

#[test]
fn upper_bound_is_exclusive() {
    let mut it = SeekableMergeIterator::new(streams([vec![1, 4, 7], vec![2, 5, 8]]));
    it.set_upper_bound(7);
    assert_eq!(it.collect::<Vec<_>>(), vec![1, 2, 4, 5]);
}

#[test]
fn seek_plus_upper_bound_walks_a_half_open_window() {
    let mut it = SeekableMergeIterator::new(streams([
        vec![1, 4, 7, 10, 13],
        vec![2, 5, 8, 11],
        vec![3, 6, 9, 12],
    ]));
    it.seek(&5);
    it.set_upper_bound(10);
    assert_eq!(it.collect::<Vec<_>>(), vec![5, 6, 7, 8, 9]);
}

#[test]
fn upper_bound_can_be_cleared_and_the_rest_still_reads() {
    let mut it = SeekableMergeIterator::new(streams([vec![1, 4, 7], vec![2, 5, 8]]));
    it.set_upper_bound(5);
    assert_eq!(it.next(), Some(1));
    assert_eq!(it.next(), Some(2));
    assert_eq!(it.next(), Some(4));
    assert_eq!(it.peek(), None);
    assert_eq!(it.next(), None);
    it.clear_upper_bound();
    assert_eq!(it.collect::<Vec<_>>(), vec![5, 7, 8]);
}

#[test]
fn upper_bound_below_the_head_exhausts_immediately() {
    let mut it = SeekableMergeIterator::new(streams([vec![10, 20], vec![30, 40]]));
    it.set_upper_bound(5);
    assert_eq!(it.peek(), None);
    assert_eq!(it.next(), None);
}

#[test]
fn peek_and_stream_counts_track_the_scan() {
    let mut it = SeekableMergeIterator::new(streams([vec![5, 9], vec![], vec![7]]));
    assert_eq!(it.num_streams(), 3);
    assert_eq!(it.live_streams(), 2);
    assert_eq!(it.peek(), Some(&5));
    it.seek(&7);
    assert_eq!(it.peek(), Some(&7));
    it.by_ref().for_each(drop);
    assert_eq!(it.live_streams(), 0);
}
