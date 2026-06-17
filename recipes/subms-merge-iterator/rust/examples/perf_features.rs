//! Per-feature bench: the base k-way `MergeIterator` plus each opt-in
//! feature (`seek-to`, `tombstones`, `dedup`, `priority`) when its Cargo
//! feature is enabled at compile time.
//!
//! The output JSON has one stage block per variant - `base_next`,
//! `seek`, `next_after_seek`, `tombstones_next`, `dedup_next`,
//! `priority_next` - so the cookbook page can fill in the per-feature
//! p99 table from one file.
//!
//! Every variant merges 16 sorted streams of ENTRIES/16 entries each.
//! The source vectors are built outside the timed loop; only the merge
//! step (`next` / `seek`) is timed.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness seek-to tombstones dedup priority"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, SubMsTimer, summarize, summary_to_json};

use subms_merge_iterator::{
    DedupEntry, DedupMergeIterator, MergeIterator, PriorityEntry, PriorityMergeIterator,
    PrioritySource, SeekableMergeIterator, TombstoneEntry, TombstoneMergeIterator,
};

const ENTRIES: usize = 50_000;
const STREAMS: usize = 16;
const SEED: u64 = 0;

const PER_STREAM: usize = ENTRIES / STREAMS;
const TOTAL: usize = PER_STREAM * STREAMS;

/// Stream `s` carries values `s, s+STREAMS, s+2*STREAMS, ...` so the 16
/// streams interleave into the dense range `0..TOTAL` with no gaps.
fn plain_streams() -> Vec<std::vec::IntoIter<u64>> {
    (0..STREAMS)
        .map(|s| {
            (0..PER_STREAM)
                .map(move |i| (s + i * STREAMS) as u64)
                .collect::<Vec<u64>>()
                .into_iter()
        })
        .collect()
}

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("merge-iterator-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("streams", &STREAMS.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("per_stream", &PER_STREAM.to_string());
    h.add_meta("subms.recipe.slug", "subms-merge-iterator");
    h.add_meta("subms.recipe.category", "storage");

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let mut iter = MergeIterator::new(plain_streams());
        let s = h
            .stage("base_next", TOTAL)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..TOTAL {
            let t0 = SubMsTimer::tick();
            let _ = iter.next();
            s.record(t0.elapsed_ns());
        }
    }

    // ---------- seek-to ----------
    {
        h.add_meta("subms.workload.feature", "seek-to");
        // Spread SEEKS targets evenly across the key range, then time a
        // seek + the next() that lands on the first key >= target. Each
        // seek advances every stream head forward, so we get one timed
        // pair per target.
        const SEEKS: usize = 4_000;
        let step = (TOTAL / SEEKS).max(1) as u64;
        let targets: Vec<u64> = (0..SEEKS).map(|i| i as u64 * step).collect();

        let mut iter = SeekableMergeIterator::new(plain_streams());
        let s_seek = h.stage("seek", SEEKS).with_kind(SubMsStageKind::HotPath);
        let mut seek_times: Vec<u64> = Vec::with_capacity(SEEKS);
        let mut after_times: Vec<u64> = Vec::with_capacity(SEEKS);
        for target in &targets {
            let t0 = SubMsTimer::tick();
            iter.seek(target);
            seek_times.push(t0.elapsed_ns());

            let t1 = SubMsTimer::tick();
            let _ = iter.next();
            after_times.push(t1.elapsed_ns());
        }
        for ns in seek_times {
            s_seek.record(ns);
        }
        let s_after = h
            .stage("next_after_seek", SEEKS)
            .with_kind(SubMsStageKind::HotPath);
        for ns in after_times {
            s_after.record(ns);
        }
    }

    // ---------- tombstones ----------
    {
        h.add_meta("subms.workload.feature", "tombstones");
        // Every 8th key in each stream is a tombstone; the rest are
        // live. next() skips shadowed + tombstoned keys per pop.
        let streams: Vec<std::vec::IntoIter<TombstoneEntry<u64, u64>>> = (0..STREAMS)
            .map(|s| {
                (0..PER_STREAM)
                    .map(move |i| {
                        let key = (s + i * STREAMS) as u64;
                        if key % 8 == 0 {
                            TombstoneEntry::tombstone(key)
                        } else {
                            TombstoneEntry::live(key, key)
                        }
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .collect();

        let mut iter = TombstoneMergeIterator::new(streams);
        let s = h
            .stage("tombstones_next", TOTAL)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..TOTAL {
            let t0 = SubMsTimer::tick();
            let yielded = iter.next();
            s.record(t0.elapsed_ns());
            if yielded.is_none() {
                break;
            }
        }
    }

    // ---------- dedup ----------
    {
        h.add_meta("subms.workload.feature", "dedup");
        // Stream s carries keys s, s+STREAMS, ... but every key is also
        // duplicated across two streams (s and s+STREAMS/2 share the
        // even keys), so next() exercises the same-key collapse path.
        let streams: Vec<std::vec::IntoIter<DedupEntry<u64, u64>>> = (0..STREAMS)
            .map(|s| {
                (0..PER_STREAM)
                    .map(move |i| {
                        // Halve the key space so adjacent streams collide.
                        let key = ((s + i * STREAMS) as u64) / 2;
                        DedupEntry::new(key, key)
                    })
                    .collect::<Vec<_>>()
                    .into_iter()
            })
            .collect();

        let mut iter = DedupMergeIterator::new(streams);
        let s = h
            .stage("dedup_next", TOTAL)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..TOTAL {
            let t0 = SubMsTimer::tick();
            let yielded = iter.next();
            s.record(t0.elapsed_ns());
            if yielded.is_none() {
                break;
            }
        }
    }

    // ---------- priority ----------
    {
        h.add_meta("subms.workload.feature", "priority");
        // Same collide-on-halved-keys shape as dedup, with an explicit
        // descending priority per source so next() runs the priority
        // tie-break on every collapsed key.
        let sources: Vec<PrioritySource<std::vec::IntoIter<PriorityEntry<u64, u64>>>> = (0
            ..STREAMS)
            .map(|s| {
                let stream = (0..PER_STREAM)
                    .map(move |i| {
                        let key = ((s + i * STREAMS) as u64) / 2;
                        PriorityEntry::new(key, key)
                    })
                    .collect::<Vec<_>>()
                    .into_iter();
                PrioritySource::new((STREAMS - s) as i32, stream)
            })
            .collect();

        let mut iter = PriorityMergeIterator::new(sources);
        let s = h
            .stage("priority_next", TOTAL)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..TOTAL {
            let t0 = SubMsTimer::tick();
            let yielded = iter.next();
            s.record(t0.elapsed_ns());
            if yielded.is_none() {
                break;
            }
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
