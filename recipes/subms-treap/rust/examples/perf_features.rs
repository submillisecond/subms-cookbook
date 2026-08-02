//! Feature classification bench. Each feature's representative op is swept
//! across three tree sizes, `classify_feature` DECIDES the category from the
//! shape of that sweep, and the decision plus a measured `p99ByStage` is
//! merge-written into `.subms/features/rust.json`.
//!
//! A treap's ops are O(log n) expected, so on a 64x size sweep a per-op feature
//! should rise by well under 2x - flat, by the classifier's reading. Anything
//! that walks the tree instead of descending it rises with n, and that is the
//! line the sweep is here to draw. `split` looks like the former and is the
//! latter.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness range-query persistent merge-split concurrent-reads"

use std::collections::BTreeMap;
use std::io::{self, Write};
use std::path::PathBuf;

use subms::{SubMsFeatureManifest, SubMsP99Source, SubMsPerfHarness, classify_feature, summarize};
use subms_treap::Treap;

const SIZES: [usize; 3] = [4_096, 32_768, 262_144];
const CANON: usize = SIZES[SIZES.len() - 1];
const SEED: u64 = 0;
/// Keyed ops per measurement. Fixed across the sweep so a slope has one cause.
const OPS: usize = 20_000;
/// Timed repeats for a whole-tree op, far too slow to run OPS times.
const BULK_REPS: usize = 32;
const BULK_WARM: usize = 8;
/// Results per range query. Held constant across the sweep so it reads the
/// DESCENT cost rather than the size of the answer.
const RANGE_TAKE: u64 = 64;
const KEY_SPACE: u64 = 1_000_000_007;

/// Key-space width that yields about `RANGE_TAKE` hits in a tree of `n` keys.
/// A fixed width would return 64x more rows at the top of the sweep and the
/// classifier would be reading the answer size, not the query.
fn range_width(n: usize) -> u64 {
    (KEY_SPACE / n as u64) * RANGE_TAKE
}

/// Scattered rather than ascending, so a descent cannot be predicted away.
fn key_at(i: usize) -> u64 {
    ((i as u64).wrapping_mul(2_654_435_761)) % 1_000_000_007
}

fn build(n: usize) -> Treap<u64, u64> {
    let mut t = Treap::new(SEED);
    for i in 0..n {
        t.insert(key_at(i), i as u64);
    }
    t
}

fn stat(h: &SubMsPerfHarness, median: bool) -> u64 {
    summarize(h)
        .stages
        .iter()
        .find(|s| s.name == "op")
        .map_or(0, |s| if median { s.p50_ns } else { s.p99_ns })
}

/// p50/p99 (ns) of `op` over a fixed OPS of keys drawn from a tree of size `n`.
fn keyed(n: usize, mut op: impl FnMut(usize), median: bool) -> u64 {
    let mut h = SubMsPerfHarness::new("treap-feature", "rust");
    let st = h.stage("op", OPS);
    for i in 0..OPS {
        let idx = (i * 7919) % n;
        st.time(|| op(idx));
    }
    stat(&h, median)
}

/// A whole-tree op. `setup` runs OUTSIDE the timed region and the first
/// `BULK_WARM` reps are discarded: measured cold, a bulk op lands its
/// first-touch cost on whichever sweep point runs first, which reads as a curve
/// that FALLS with size - the opposite of the structural signal.
fn bulk<T>(mut setup: impl FnMut() -> T, mut op: impl FnMut(&mut T), median: bool) -> u64 {
    let mut input = setup();
    for _ in 0..BULK_WARM {
        op(&mut input);
    }
    let mut h = SubMsPerfHarness::new("treap-feature", "rust");
    let st = h.stage("op", BULK_REPS);
    for _ in 0..BULK_REPS {
        st.time(|| op(&mut input));
    }
    stat(&h, median)
}

/// Sweeps and PRINTS the curve. A non-monotonic or ratio-compressed sweep
/// classifies flat, and the only way to catch one is to look at the rows.
fn sweep(label: &str, mut at: impl FnMut(usize) -> u64) -> Vec<(usize, u64)> {
    let rows: Vec<(usize, u64)> = SIZES.iter().map(|&n| (n, at(n))).collect();
    eprintln!("sweep {label}: {rows:?}");
    rows
}

fn main() -> io::Result<()> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(".subms")
        .join("features")
        .join("rust.json");
    let existing = std::fs::read_to_string(&path).unwrap_or_default();
    let mut manifest = SubMsFeatureManifest::load_str("rust", &existing);
    // Stamp the box these numbers came from. The bench runs wherever it is
    // invoked, so an unstamped manifest is indistinguishable from a fleet
    // capture; the renderer will not publish one it cannot attribute.
    let (source, instance) = SubMsP99Source::from_env();
    manifest.set_p99_source(source, instance.as_deref());

    // The baseline: a base-treap lookup at the canonical size. A feature landing
    // at or under this costs nothing on the read path.
    let base = build(CANON);
    let base_p50 = keyed(CANON, |i| _ = base.get(&key_at(i)), true);
    eprintln!("base get p50: {base_p50}ns");

    // ---------- range-query: an in-order walk between two bounds ----------
    #[cfg(feature = "range-query")]
    {
        use subms_treap::RangeBound;
        // The window is sized to yield a constant number of rows at every sweep
        // point. Bounded rather than lazily truncated because the Java port's
        // range query materialises its whole window - a `take(64)` on a lazy
        // iterator has no equivalent there, and the two ports have to measure
        // the same thing.
        let sw = sweep("range-query/range", |n| {
            let t = build(n);
            let w = range_width(n);
            keyed(
                n,
                |i| {
                    let from = key_at(i);
                    let to = from.saturating_add(w);
                    _ = t
                        .range(RangeBound::Inclusive(&from), RangeBound::Inclusive(&to))
                        .count();
                },
                true,
            )
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let t = build(CANON);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "range_scan".to_string(),
            keyed(
                CANON,
                |i| {
                    let from = key_at(i);
                    let to = from.saturating_add(range_width(CANON));
                    _ = t
                        .range(RangeBound::Inclusive(&from), RangeBound::Inclusive(&to))
                        .count();
                },
                false,
            ),
        );
        manifest.set_feature("range-query", cat, &p99, &reason);
    }

    // ---------- persistent: path-copying insert, old version stays valid ----------
    #[cfg(feature = "persistent")]
    {
        use subms_treap::PersistentTreap;
        // `insert` returns a NEW treap sharing everything off the copied path,
        // so the cost is the path length - O(log n), which should read flat.
        let sw = sweep("persistent/insert", |n| {
            let mut p = PersistentTreap::new(SEED);
            for i in 0..n {
                p = p.insert(key_at(i), i as u64);
            }
            keyed(n, |i| _ = p.insert(key_at(i), i as u64), true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p = PersistentTreap::new(SEED);
        for i in 0..CANON {
            p = p.insert(key_at(i), i as u64);
        }
        let mut p99 = BTreeMap::new();
        p99.insert(
            "insert".to_string(),
            keyed(CANON, |i| _ = p.insert(key_at(i), i as u64), false),
        );
        p99.insert(
            "get".to_string(),
            keyed(CANON, |i| _ = p.get(&key_at(i)), false),
        );
        p99.insert(
            "remove".to_string(),
            keyed(CANON, |i| _ = p.remove(&key_at(i)), false),
        );
        manifest.set_feature("persistent", cat, &p99, &reason);
    }

    // ---------- merge-split: split at a pivot, merge two ordered halves ----------
    #[cfg(feature = "merge-split")]
    {
        use subms_treap::SplittableTreap;
        // Timed as a split-then-merge ROUND TRIP, because `split` consumes the
        // treap: rebuilding one per rep would put an O(n log n) build inside the
        // timed region and the figure would be the build. A round trip restores
        // the original, so the input is set up once and every rep does identical
        // work.
        //
        // The sweep classifies this structural, and the reason is in `split`
        // rather than in `split_node`: the descent is O(log n), but split then
        // calls `count()` on BOTH halves to fill in their lengths, and that is a
        // full traversal. An O(log n) op with an O(n) bookkeeping tail.
        let make = |n: usize| {
            let mut t = SplittableTreap::new(SEED);
            for i in 0..n {
                t.insert(key_at(i), i as u64);
            }
            Some(t)
        };
        let round_trip = |slot: &mut Option<SplittableTreap<u64, u64>>| {
            let t = slot.take().expect("round trip restores the treap");
            let (l, r) = t.split(&(KEY_SPACE / 2));
            *slot = Some(SplittableTreap::merge(l, r));
        };
        let sw = sweep("merge-split/split+merge", |n| {
            bulk(|| make(n), round_trip, true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let mut p99 = BTreeMap::new();
        p99.insert(
            "split_merge".to_string(),
            bulk(|| make(CANON), round_trip, false),
        );
        manifest.set_feature("merge-split", cat, &p99, &reason);
    }

    // ---------- concurrent-reads: a flattened immutable snapshot ----------
    #[cfg(feature = "concurrent-reads")]
    {
        use subms_treap::TreapSnapshot;
        // `from_treap` flattens the tree into a sorted Vec, so it is O(n) and the
        // sweep says so. Lookups on the result are a binary search over that Vec,
        // which is the point: readers pay O(log n) with no tree pointers and no
        // coordination with the writer.
        let sw = sweep("concurrent-reads/snapshot", |n| {
            let t = build(n);
            bulk(|| (), |()| _ = TreapSnapshot::from_treap(&t), true)
        });
        let (cat, reason) = classify_feature(&sw, Some(base_p50), None);

        let t = build(CANON);
        let snap = TreapSnapshot::from_treap(&t);
        let mut p99 = BTreeMap::new();
        p99.insert(
            "snapshot".to_string(),
            bulk(|| (), |()| _ = TreapSnapshot::from_treap(&t), false),
        );
        p99.insert(
            "lookup_on_snapshot".to_string(),
            keyed(CANON, |i| _ = snap.get(&key_at(i)), false),
        );
        manifest.set_feature("concurrent-reads", cat, &p99, &reason);
    }

    std::fs::create_dir_all(path.parent().unwrap())?;
    std::fs::write(&path, manifest.to_json())?;
    io::stdout().write_all(manifest.to_json().as_bytes())?;
    Ok(())
}
