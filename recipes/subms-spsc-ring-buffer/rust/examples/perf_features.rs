//! Per-feature bench: runs a fixed 50k-op workload against the base
//! SPSC ring plus each opt-in feature (`bulk`, `wait-strategies`,
//! `mpsc-fan-in`, `mpmc-disruptor`, `metrics`) when its Cargo feature is
//! enabled at compile time.
//!
//! The output JSON carries one stage block per feature variant - e.g.
//! `base_enqueue`, `bulk_enqueue`, `disruptor_publish` - so the cookbook
//! page fills its per-feature p99 table from one JSON file.
//!
//! Everything runs single-threaded: one operation at a time, timed in
//! isolation, so the numbers are per-op latency rather than two-thread
//! throughput. Rings are sized so the base push/pop passes never go
//! full / empty, and the concurrent-shaped features are driven one op at
//! a time against an uncontended counter.
//!
//! Run:
//!   cargo run --release --example perf_features \
//!       --features "harness bulk wait-strategies mpsc-fan-in mpmc-disruptor metrics"

use std::io::{self, Write};

use subms::{SubMsPerfHarness, SubMsStageKind, summarize, summary_to_json};

const ENTRIES: usize = 50_000;
const SEED: u64 = 0;
const BULK_BATCH: usize = 32;

fn main() -> io::Result<()> {
    let mut h = SubMsPerfHarness::new("spsc-ring-buffer-features", "rust");
    h.input("entries", &ENTRIES.to_string());
    h.input("seed", &SEED.to_string());
    h.add_meta("subms.recipe.slug", "subms-spsc-ring-buffer");
    h.add_meta("subms.recipe.category", "concurrency");

    base(&mut h);

    #[cfg(feature = "bulk")]
    bulk(&mut h);

    #[cfg(feature = "wait-strategies")]
    wait_strategies(&mut h);

    #[cfg(feature = "mpsc-fan-in")]
    mpsc_fan_in(&mut h);

    #[cfg(feature = "mpmc-disruptor")]
    mpmc_disruptor(&mut h);

    #[cfg(feature = "metrics")]
    metrics(&mut h);

    let summary = summarize(&h);
    let mut stdout = io::stdout();
    summary_to_json(&summary, &mut stdout)?;
    writeln!(stdout)?;
    Ok(())
}

// ---------- base ----------
// Capacity is rounded up past ENTRIES so every push lands and every pop
// returns Some; the timed path is the uncontested fast path on each side.
fn base(h: &mut SubMsPerfHarness) {
    use subms_spsc_ring_buffer::SpscRingBuffer;
    h.add_meta("subms.workload.feature", "base");

    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u64>(ENTRIES);

    let stage = h
        .stage("base_enqueue", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for i in 0..ENTRIES as u64 {
        stage.time(|| {
            let _ = tx.try_push(i);
        });
    }

    let stage = h
        .stage("base_dequeue", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..ENTRIES {
        stage.time(|| {
            let _ = rx.try_pop();
        });
    }
}

// ---------- bulk ----------
// One timed call moves a BULK_BATCH-wide slice, so the per-item atomic
// load + cache check is amortised behind a single fence. The stage count
// is the number of batch calls, not the item count.
#[cfg(feature = "bulk")]
fn bulk(h: &mut SubMsPerfHarness) {
    use subms_spsc_ring_buffer::SpscRingBuffer;
    h.add_meta("subms.workload.feature", "bulk");

    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u64>(ENTRIES);
    let batch: Vec<u64> = (0..BULK_BATCH as u64).collect();
    let calls = ENTRIES / BULK_BATCH;

    let stage = h
        .stage("enqueue_bulk", calls)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..calls {
        stage.time(|| {
            let _ = tx.try_enqueue_bulk(&batch);
        });
    }

    let mut out = [0u64; BULK_BATCH];
    let stage = h
        .stage("dequeue_bulk", calls)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..calls {
        stage.time(|| {
            let _ = rx.try_dequeue_bulk(&mut out);
        });
    }
}

// ---------- wait-strategies ----------
// BusySpin wrappers, single-threaded round-trip: push then pop. The slot
// is always immediately available so the strategy's wait() never fires;
// this measures the blocking-wrapper overhead on the fast path.
#[cfg(feature = "wait-strategies")]
fn wait_strategies(h: &mut SubMsPerfHarness) {
    use subms_spsc_ring_buffer::{
        BlockingSpscConsumer, BlockingSpscProducer, BusySpin, SpscRingBuffer,
    };
    h.add_meta("subms.workload.feature", "wait-strategies");

    let (tx, rx) = SpscRingBuffer::with_capacity::<u64>(1024);
    let mut p = BlockingSpscProducer::new(tx, BusySpin);
    let mut c = BlockingSpscConsumer::new(rx, BusySpin);

    let stage = h
        .stage("wait_enqueue", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for i in 0..ENTRIES as u64 {
        stage.time(|| p.push(i));
        // Drain immediately so the next push never blocks on a full ring.
        c.pop();
    }

    // Separate pass to time the consumer fast path in isolation.
    let stage = h
        .stage("wait_dequeue", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for i in 0..ENTRIES as u64 {
        p.push(i);
        stage.time(|| {
            let _ = c.pop();
        });
    }
}

// ---------- mpsc-fan-in ----------
// N independent SPSC rings, one consumer round-robining across them.
// Producers are pushed round-robin so each ring fills evenly; the
// consumer then drains via its cursor. Timed one op at a time.
#[cfg(feature = "mpsc-fan-in")]
fn mpsc_fan_in(h: &mut SubMsPerfHarness) {
    use subms_spsc_ring_buffer::MpscFanIn;
    h.add_meta("subms.workload.feature", "mpsc-fan-in");

    const PRODUCERS: usize = 4;
    let per_ring = ENTRIES / PRODUCERS;
    let (mut producers, mut consumer) =
        MpscFanIn::with_capacity::<u64>(PRODUCERS, per_ring.next_power_of_two());

    let stage = h
        .stage("fanin_enqueue", per_ring * PRODUCERS)
        .with_kind(SubMsStageKind::HotPath);
    for i in 0..per_ring {
        for p in producers.iter_mut() {
            let v = i as u64;
            stage.time(|| {
                let _ = p.try_push(v);
            });
        }
    }

    let stage = h
        .stage("fanin_dequeue", per_ring * PRODUCERS)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..per_ring * PRODUCERS {
        stage.time(|| {
            let _ = consumer.try_pop();
        });
    }
}

// ---------- mpmc-disruptor ----------
// Single producer, single consumer, publish then consume interleaved so
// the ring never fills (the producer gates behind the slowest consumer).
// Measures the CAS-claim + publish path and the sequence-barrier consume
// path one op at a time.
#[cfg(feature = "mpmc-disruptor")]
fn mpmc_disruptor(h: &mut SubMsPerfHarness) {
    use subms_spsc_ring_buffer::MpmcDisruptor;
    h.add_meta("subms.workload.feature", "mpmc-disruptor");

    let (producer, mut consumers) = MpmcDisruptor::with_consumers::<u64>(1024, 1);
    let mut consumer = consumers.remove(0);

    // Create both stages up front, then borrow each by name per iteration so
    // publish + consume interleave without filling the ring.
    h.stage("publish", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    h.stage("consume", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for i in 0..ENTRIES as u64 {
        h.stage_mut("publish").unwrap().time(|| {
            let _ = producer.try_publish(i);
        });
        h.stage_mut("consume").unwrap().time(|| {
            let _ = consumer.try_consume();
        });
    }
}

// ---------- metrics ----------
// Instrumented wrap; one fetch_add per op on top of the base push/pop.
#[cfg(feature = "metrics")]
fn metrics(h: &mut SubMsPerfHarness) {
    use subms_spsc_ring_buffer::{InstrumentedSpsc, SpscRingBuffer};
    h.add_meta("subms.workload.feature", "metrics");

    let (tx, rx) = SpscRingBuffer::with_capacity::<u64>(ENTRIES);
    let (mut tx, mut rx, _m) = InstrumentedSpsc::wrap(tx, rx);

    let stage = h
        .stage("metrics_enqueue", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for i in 0..ENTRIES as u64 {
        stage.time(|| {
            let _ = tx.try_push(i);
        });
    }

    let stage = h
        .stage("metrics_dequeue", ENTRIES)
        .with_kind(SubMsStageKind::HotPath);
    for _ in 0..ENTRIES {
        stage.time(|| {
            let _ = rx.try_pop();
        });
    }
}
