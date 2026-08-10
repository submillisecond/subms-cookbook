use super::*;

#[test]
fn merges_three_streams() {
    let streams: Vec<std::vec::IntoIter<i32>> = vec![
        vec![1, 4, 7].into_iter(),
        vec![2, 5, 8].into_iter(),
        vec![3, 6, 9].into_iter(),
    ];
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged, (1..=9).collect::<Vec<_>>());
}

#[test]
fn handles_empty_streams() {
    let streams: Vec<std::vec::IntoIter<i32>> = vec![
        vec![].into_iter(),
        vec![5, 10].into_iter(),
        vec![].into_iter(),
    ];
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged, vec![5, 10]);
}

#[test]
fn handles_duplicates_across_streams() {
    let streams: Vec<std::vec::IntoIter<i32>> =
        vec![vec![1, 2, 3].into_iter(), vec![2, 3, 4].into_iter()];
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged, vec![1, 2, 2, 3, 3, 4]);
}

#[test]
fn single_stream_passes_through() {
    let streams: Vec<std::vec::IntoIter<i32>> = vec![vec![1, 2, 3].into_iter()];
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged, vec![1, 2, 3]);
}

#[test]
fn no_streams_yields_empty() {
    let streams: Vec<std::vec::IntoIter<i32>> = vec![];
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert!(merged.is_empty());
}

#[test]
fn ten_streams_thousand_elements_each() {
    let streams: Vec<std::vec::IntoIter<i32>> = (0..10)
        .map(|s| {
            (0..1000)
                .map(move |i| s + 10 * i)
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged.len(), 10_000);
    for w in merged.windows(2) {
        assert!(w[0] <= w[1], "sorted");
    }
}

#[test]
fn one_long_one_short_stream() {
    let s1: std::vec::IntoIter<i32> = vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10].into_iter();
    let s2: std::vec::IntoIter<i32> = vec![5].into_iter();
    let merged: Vec<_> = MergeIterator::new(vec![s1, s2]).collect();
    assert_eq!(merged, vec![1, 2, 3, 4, 5, 5, 6, 7, 8, 9, 10]);
}

#[test]
fn streams_with_negative_values() {
    let streams: Vec<std::vec::IntoIter<i32>> =
        vec![vec![-5, -1, 0].into_iter(), vec![-3, 2, 4].into_iter()];
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged, vec![-5, -3, -1, 0, 2, 4]);
}

#[test]
fn merge_preserves_total_count() {
    let n_streams = 5i32;
    let per = 200i32;
    let streams: Vec<std::vec::IntoIter<i32>> = (0..n_streams)
        .map(|s| {
            (0..per)
                .map(move |i| s * 1000 + i)
                .collect::<Vec<_>>()
                .into_iter()
        })
        .collect();
    let merged: Vec<_> = MergeIterator::new(streams).collect();
    assert_eq!(merged.len() as i32, n_streams * per);
}

#[test]
fn peek_shows_the_next_value_without_consuming_it() {
    let streams: Vec<std::vec::IntoIter<i32>> =
        vec![vec![4, 9].into_iter(), vec![1, 6].into_iter()];
    let mut it = MergeIterator::new(streams);
    assert_eq!(it.peek(), Some(&1));
    assert_eq!(it.peek(), Some(&1));
    assert_eq!(it.next(), Some(1));
    assert_eq!(it.peek(), Some(&4));
}

#[test]
fn live_streams_tracks_exhaustion() {
    let streams: Vec<std::vec::IntoIter<i32>> = vec![
        vec![1].into_iter(),
        vec![2, 3].into_iter(),
        vec![].into_iter(),
    ];
    let mut it = MergeIterator::new(streams);
    assert_eq!(it.num_streams(), 3, "empty source still counts as declared");
    assert_eq!(it.live_streams(), 2, "the empty source never gets a head");
    it.next();
    assert_eq!(it.live_streams(), 1);
    it.by_ref().for_each(drop);
    assert_eq!(it.live_streams(), 0);
    assert_eq!(it.peek(), None);
}
