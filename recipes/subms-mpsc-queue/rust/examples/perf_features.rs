//! Per-feature bench: runs the same 50k-entry workload against the base
//! `MpscQueue`, plus each opt-in feature (`mpmc`, `bounded`, `batch`,
//! `metrics`) when its Cargo feature is enabled at compile time.
//!
//! The output JSON has one stage block per feature variant - e.g.
//! `base_enqueue`, `mpmc_enqueue`, `bounded_dequeue`, `batch_drain` - so
//! the cookbook page can fill in the per-feature p99 table from a single
//! JSON file.
//!
//! Concurrent variants (`mpmc`) are driven single-threaded here: the
//! point is the per-op latency of the enqueue / dequeue path, not the
//! contention curve. The `affinity` feature has no per-op hot-path cost
//! (it pins a thread once at setup), so it is timed once as
//! `affinity_set` rather than per-op.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness mpmc bounded batch metrics"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};
use subms_mpsc_queue::{MpscQueue, PopResult};

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("mpsc-queue-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-mpsc-queue");
    h.add_meta("subms.recipe.category", "concurrency");

    // ---------- base ----------
    {
        h.add_meta("subms.workload.feature", "base");
        let q: MpscQueue<u64> = MpscQueue::new();
        let stage = h
            .stage("base_enqueue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u64 {
            stage.time(|| q.push(i));
        }
        let mut q = q;
        let stage = h
            .stage("base_dequeue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| match q.try_pop() {
                PopResult::Some(v) => {
                    let _ = v;
                }
                PopResult::Empty | PopResult::Inconsistent => {}
            });
        }
    }

    // ---------- mpmc ----------
    #[cfg(feature = "mpmc")]
    {
        use subms_mpsc_queue::MpmcQueue;
        h.add_meta("subms.workload.feature", "mpmc");
        let q: MpmcQueue<u64> = MpmcQueue::new(ENTRIES);
        let cap = q.capacity();
        let stage = h
            .stage("mpmc_enqueue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u64 {
            if i as usize >= cap {
                break;
            }
            stage.time(|| {
                let _ = q.try_enqueue(i);
            });
        }
        let stage = h
            .stage("mpmc_dequeue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| {
                let _ = q.try_dequeue();
            });
        }
    }

    // ---------- bounded ----------
    #[cfg(feature = "bounded")]
    {
        use subms_mpsc_queue::BoundedMpscQueue;
        h.add_meta("subms.workload.feature", "bounded");
        let q: BoundedMpscQueue<u64> = BoundedMpscQueue::new(ENTRIES);
        let cap = q.capacity();
        let stage = h
            .stage("bounded_enqueue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u64 {
            if i as usize >= cap {
                break;
            }
            stage.time(|| {
                let _ = q.try_enqueue(i);
            });
        }
        let mut q = q;
        let stage = h
            .stage("bounded_dequeue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| {
                let _ = q.try_dequeue();
            });
        }
    }

    // ---------- batch ----------
    #[cfg(feature = "batch")]
    {
        use subms_mpsc_queue::BatchMpscQueue;
        h.add_meta("subms.workload.feature", "batch");
        const BATCH: usize = 256;
        let q: BatchMpscQueue<u64> = BatchMpscQueue::new();
        for i in 0..ENTRIES as u64 {
            q.push(i);
        }
        let mut q = q;
        let mut buf: Vec<Option<u64>> = (0..BATCH).map(|_| None).collect();
        let batches = ENTRIES.div_ceil(BATCH);
        let stage = h
            .stage("batch_drain", batches)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..batches {
            stage.time(|| {
                let n = q.try_dequeue_batch(&mut buf);
                for slot in buf.iter_mut().take(n) {
                    let _ = slot.take();
                }
            });
        }
    }

    // ---------- metrics ----------
    #[cfg(feature = "metrics")]
    {
        use subms_mpsc_queue::MetricsMpscQueue;
        h.add_meta("subms.workload.feature", "metrics");
        let q: MetricsMpscQueue<u64> = MetricsMpscQueue::new();
        let stage = h
            .stage("metrics_enqueue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for i in 0..ENTRIES as u64 {
            stage.time(|| q.push(i));
        }
        let mut q = q;
        let stage = h
            .stage("metrics_dequeue", ENTRIES)
            .with_kind(SubMsStageKind::HotPath);
        for _ in 0..ENTRIES {
            stage.time(|| match q.try_pop() {
                PopResult::Some(v) => {
                    let _ = v;
                }
                PopResult::Empty | PopResult::Inconsistent => {}
            });
        }
    }

    // ---------- affinity ----------
    #[cfg(feature = "affinity")]
    {
        use subms_mpsc_queue::set_affinity;
        h.add_meta("subms.workload.feature", "affinity");
        let stage = h
            .stage("affinity_set", 1)
            .with_kind(SubMsStageKind::OneShot);
        stage.time(|| {
            let _ = set_affinity(&[0]);
        });
    }

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}
