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
fn seek_with_some_exhausted_streams() {
    // Stream 0 ends well before seek target.
    let mut it = SeekableMergeIterator::new(streams([vec![1, 2, 3], vec![10, 20, 30]]));
    it.seek(&15);
    let rest: Vec<_> = it.collect();
    assert_eq!(rest, vec![20, 30]);
}
