//! `SubMsRecipe` impl.

use std::time::Instant;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe};

use crate::MergeIterator;

pub struct MergeIteratorRecipe;

impl SubMsRecipe for MergeIteratorRecipe {
    fn name(&self) -> &str {
        "merge-iterator"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let n_streams = 16usize;
        let per_stream = params.entries / n_streams;

        // Build N sorted ascending streams. stream s has values s, s+N, s+2N, ...
        let streams: Vec<std::vec::IntoIter<u64>> = (0..n_streams)
            .map(|s| {
                (0..per_stream)
                    .map(move |i| (s + i * n_streams) as u64)
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .collect();

        let mut iter = MergeIterator::new(streams);
        let total = n_streams * per_stream;
        let s_next = h.stage("next", total);
        for _ in 0..total {
            let t0 = Instant::now();
            let _ = iter.next();
            s_next.record(t0.elapsed().as_nanos() as u64);
        }

        h.add_meta("streams", &n_streams.to_string());
        h.add_meta("per_stream", &per_stream.to_string());
    }
}
