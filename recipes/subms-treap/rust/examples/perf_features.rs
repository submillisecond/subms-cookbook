//! Per-feature bench: runs a 50k-entry workload against the base
//! `Treap`, plus each opt-in feature (`range-query`, `persistent`,
//! `merge-split`, `concurrent-reads`) when its Cargo feature is
//! enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_insert`, `range`, `persistent_insert`, `split`, etc. - so
//! the cookbook page can fill in the per-feature p99 table from a
//! single JSON file.
//!
//! Keys are `u64` so the ordered-structure features (range scans,
//! splits) exercise real BST-on-key ordering rather than the
//! lexicographic order of stringified keys. Each stage seeds its own
//! `SubMsLcg` from `SEED` so the key universe is reproducible.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness range-query persistent merge-split concurrent-reads"

use std::io::{self, Write};

use subms::{SubMsLcg, SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};
use subms_treap::Treap;

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;

fn keys(seed: u64, count: usize) -> Vec<u64> {
    let mut rng = SubMsLcg::new(seed);
    (0..count).map(|_| rng.next_u32() as u64).collect()
}

fn main() -> io::Result<()> {
    // The pointer/Rc-backed feature treaps recurse to tree depth on
    // insert/split/merge; a 50k-entry random treap can spike well past
    // the 1 MB default Windows main-thread stack. Run on a worker
    // thread with a generous stack so deep-but-bounded recursion stays
    // within budget without touching the library's recursion shape.
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(run)
        .expect("spawn worker thread")
        .join()
        .expect("worker thread panicked")
}

fn run() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("treap-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-treap");
    h.add_meta("subms.recipe.category", "ordered-index");

    let key_set = keys(SEED, ENTRIES);

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let mut t: Treap<u64, u64> = Treap::with_capacity(SEED, ENTRIES);
        let stage = h
            .stage("base_insert", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for &k in &key_set {
            stage.time(|| {
                t.insert(k, k);
            });
        }
        let stage = h
            .stage("base_get", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for &k in &key_set {
            stage.time(|| {
                let _ = t.get(&k);
            });
        }
    }

    // ---------- range-query ----------
    #[cfg(feature = "range-query")]
    {
        use subms_treap::RangeBound;
        h.add_meta("subms.workload.feature", "range-query");

        // Dense keys 0..ENTRIES so a fixed-width window always lands on a
        // populated stretch - otherwise sparse u32 keys make almost every
        // window empty and `next` never advances.
        let mut t: Treap<u64, u64> = Treap::with_capacity(SEED, ENTRIES);
        for k in 0..ENTRIES as u64 {
            t.insert(k, k);
        }

        // `range` times the boundary descent + first element pull: the
        // O(log T) locate cost. `next` times each subsequent in-order
        // step across a fixed-width populated window so it isolates the
        // amortised O(1) advance from the one-off seek.
        const WINDOW: u64 = 256;
        let max_start = ENTRIES as u64 - WINDOW - 1;

        let stage = h.stage("range", ENTRIES).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(SEED ^ 0x9e37);
        for _ in 0..ENTRIES {
            let from = (rng.next_u32() as u64) % max_start;
            let to = from + WINDOW;
            stage.time(|| {
                let mut it = t.range(RangeBound::Inclusive(&from), RangeBound::Inclusive(&to));
                let _ = it.next();
            });
        }

        // Record exactly ENTRIES timed `next()` advances, opening a fresh
        // window whenever the current iterator runs dry. Each window holds
        // WINDOW+1 dense keys, so progress is guaranteed.
        let stage = h.stage("next", ENTRIES).with_kind(SubMsStageKind::HotPath);
        let mut rng = SubMsLcg::new(SEED ^ 0x9e37);
        let mut recorded = 0usize;
        'outer: loop {
            let from = (rng.next_u32() as u64) % max_start;
            let to = from + WINDOW;
            let mut it = t.range(RangeBound::Inclusive(&from), RangeBound::Inclusive(&to));
            let _ = it.next();
            loop {
                let advanced = stage.time(|| it.next().is_some());
                if !advanced {
                    break;
                }
                recorded += 1;
                if recorded >= ENTRIES {
                    break 'outer;
                }
            }
        }
    }

    // ---------- persistent ----------
    #[cfg(feature = "persistent")]
    {
        use subms_treap::PersistentTreap;
        h.add_meta("subms.workload.feature", "persistent");

        // `insert` returns a NEW version each call; chaining them grows
        // a version chain so the path-copy cost is measured against a
        // realistically-sized tree.
        let mut t: PersistentTreap<u64, u64> = PersistentTreap::new(SEED);
        let stage = h
            .stage("persistent_insert", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for &k in &key_set {
            t = stage.time(|| t.insert(k, k));
        }

        let final_version = t;
        let stage = h
            .stage("persistent_get", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for &k in &key_set {
            stage.time(|| {
                let _ = final_version.get(&k);
            });
        }
    }

    // ---------- merge-split ----------
    #[cfg(feature = "merge-split")]
    {
        use subms_treap::SplittableTreap;
        h.add_meta("subms.workload.feature", "merge-split");

        // split + merge both consume the treap and hand it back, so the
        // round-trip is: split at a pivot, then merge the halves back.
        // `split` recomputes both halves' lengths with a full traversal,
        // so each round-trip is O(N) at this 50k size - we run a bounded
        // number of rounds (still a healthy sample for a stable p99)
        // rather than ENTRIES, which would be quadratic. Pivots walk
        // across the key space so the split point varies per round.
        const MS_ROUNDS: usize = 2_000;

        let mut tree: SplittableTreap<u64, u64> = SplittableTreap::new(SEED);
        for &k in &key_set {
            tree.insert(k, k);
        }

        let pivots = keys(SEED ^ 0x5151, MS_ROUNDS);

        let mut split_samples = Vec::with_capacity(MS_ROUNDS);
        let mut merge_samples = Vec::with_capacity(MS_ROUNDS);

        for &pivot in &pivots {
            let t0 = std::time::Instant::now();
            let (lo, hi) = tree.split(&pivot);
            split_samples.push(t0.elapsed().as_nanos() as u64);

            let t1 = std::time::Instant::now();
            tree = SplittableTreap::merge(lo, hi);
            merge_samples.push(t1.elapsed().as_nanos() as u64);
        }

        let stage = h
            .stage("split", split_samples.len())
            .with_kind(SubMsStageKind::BatchOp);
        for ns in split_samples {
            stage.record(ns);
        }
        let stage = h
            .stage("merge", merge_samples.len())
            .with_kind(SubMsStageKind::BatchOp);
        for ns in merge_samples {
            stage.record(ns);
        }
    }

    // ---------- concurrent-reads ----------
    #[cfg(feature = "concurrent-reads")]
    {
        use subms_treap::TreapSnapshot;
        h.add_meta("subms.workload.feature", "concurrent-reads");

        let mut t: Treap<u64, u64> = Treap::with_capacity(SEED, ENTRIES);
        for &k in &key_set {
            t.insert(k, k);
        }

        // `snapshot` is a one-shot O(N) capture; time a handful of
        // captures so the stage has more than a single sample but the
        // count stays small (it is not a per-op hot path).
        let stage = h.stage("snapshot", 32).with_kind(SubMsStageKind::BatchOp);
        let mut snap = TreapSnapshot::from_treap(&t);
        for _ in 0..32 {
            snap = stage.time(|| TreapSnapshot::from_treap(&t));
        }

        let stage = h
            .stage("get_on_snapshot", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for &k in &key_set {
            stage.time(|| {
                let _ = snap.get(&k);
            });
        }
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
