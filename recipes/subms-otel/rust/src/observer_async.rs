//! Asynchronous OpenTelemetry observer. `on_record` does one push onto a
//! hand-rolled single-producer / single-consumer ring (~150 ns target) and
//! returns; a background drain thread builds histogram instruments and emits
//! in batches at a configurable interval.
//!
//! Back-pressure policy is drop-newest: if the ring is full, we drop the
//! sample AND increment a `subms.otel.dropped_total` counter that gets emitted
//! alongside the latency histogram.
//!
//! The async observer also publishes:
//!
//! - `subms.bench.ops_total` - counter, +1 per `on_record`.
//! - `subms.bench.in_flight` - observable gauge, current ring depth.
//! - `subms.otel.exemplars_kept_total` - counter, +1 per exemplar retained.

use std::cell::UnsafeCell;
use std::mem::MaybeUninit;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use opentelemetry::metrics::{Counter, Histogram, Meter};

use subms::{ObservationCtx, SubMsBenchSummary, SubMsObserver, SubMsStageKind};

use crate::bridge::{attributes_from_ctx, histogram_for_stage, push_meta_attrs};
use crate::drift::ReferenceDivergenceRecorder;
#[cfg(feature = "exemplars")]
use crate::exemplars::ExemplarReservoir;
use crate::{
    DROPPED_TOTAL_COUNTER_NAME, EXEMPLARS_KEPT_COUNTER_NAME, IN_FLIGHT_GAUGE_NAME,
    OPS_TOTAL_COUNTER_NAME,
};

/// Default ring capacity. 64K records buffered between drain passes is plenty
/// for sub-millisecond stages emitting at MHz rates as long as the drain
/// interval keeps up. Rounded up to the next power of two if not already.
pub const DEFAULT_CHANNEL_CAPACITY: usize = 65_536;

/// Default drain cadence: every 100ms.
pub const DEFAULT_DRAIN_INTERVAL: Duration = Duration::from_millis(100);

struct Sample {
    stage: Arc<str>,
    workload: Arc<str>,
    lang: Arc<str>,
    kind: SubMsStageKind,
    ns: u64,
}

#[derive(Default, Clone)]
struct SummaryCache {
    inputs: std::collections::BTreeMap<String, String>,
    meta: std::collections::BTreeMap<String, String>,
}

/// Single-producer / single-consumer ring. Producer = harness's record path
/// (`on_record` is called from a single thread per harness instance); consumer
/// = the drain worker.
///
/// SAFETY (invariants):
/// - Producer writes only at `head & mask`; consumer reads only at `tail & mask`.
/// - Producer publishes by Release-storing `head`; consumer reads it Acquire.
/// - Tail is symmetric. A slot is owned exclusively by whichever side is
///   permitted to touch it under the head/tail gap; the other side never
///   races.
/// - `capacity` is a power of two; `mask = capacity - 1`.
struct SpscRing {
    buf: Box<[UnsafeCell<MaybeUninit<Sample>>]>,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl Sync for SpscRing {}

impl SpscRing {
    fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        let cap = capacity.next_power_of_two();
        let mut buf = Vec::with_capacity(cap);
        for _ in 0..cap {
            buf.push(UnsafeCell::new(MaybeUninit::uninit()));
        }
        Self {
            buf: buf.into_boxed_slice(),
            mask: cap - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn capacity(&self) -> usize {
        self.mask + 1
    }

    fn len(&self) -> usize {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    /// Push a sample. Returns `Err(sample)` if the ring is full.
    fn push(&self, sample: Sample) -> Result<(), Sample> {
        let head = self.head.load(Ordering::Relaxed);
        let tail = self.tail.load(Ordering::Acquire);
        if head.wrapping_sub(tail) >= self.capacity() {
            return Err(sample);
        }
        let slot = &self.buf[head & self.mask];
        unsafe {
            (*slot.get()).write(sample);
        }
        self.head.store(head.wrapping_add(1), Ordering::Release);
        Ok(())
    }

    /// Pop a sample. Returns `None` if the ring is empty.
    fn pop(&self) -> Option<Sample> {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let slot = &self.buf[tail & self.mask];
        let sample = unsafe { (*slot.get()).assume_init_read() };
        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        Some(sample)
    }
}

impl Drop for SpscRing {
    fn drop(&mut self) {
        while self.pop().is_some() {}
    }
}

/// Async observer. See module docs for the back-pressure contract.
pub struct OtelObserverAsync {
    meter: Meter,
    ring: Arc<SpscRing>,
    dropped: Arc<AtomicU64>,
    summary_cache: Arc<Mutex<SummaryCache>>,
    shutdown: Arc<AtomicBool>,
    worker: Mutex<Option<JoinHandle<()>>>,
    ops_counter: Counter<u64>,
    exemplars_kept: Counter<u64>,
    drift: ReferenceDivergenceRecorder,
    #[cfg(feature = "exemplars")]
    exemplars: Option<Arc<ExemplarReservoir>>,
}

/// Builder for [`OtelObserverAsync`].
pub struct OtelObserverAsyncBuilder {
    meter: Meter,
    capacity: usize,
    drain_interval: Duration,
    #[cfg(feature = "exemplars")]
    exemplars: Option<Arc<ExemplarReservoir>>,
}

impl OtelObserverAsync {
    /// Build with defaults: 64K ring, 100ms drain interval.
    pub fn new(meter: Meter) -> Self {
        Self::builder(meter).build()
    }

    /// Start configuring.
    pub fn builder(meter: Meter) -> OtelObserverAsyncBuilder {
        OtelObserverAsyncBuilder {
            meter,
            capacity: DEFAULT_CHANNEL_CAPACITY,
            drain_interval: DEFAULT_DRAIN_INTERVAL,
            #[cfg(feature = "exemplars")]
            exemplars: None,
        }
    }

    /// Read the current drop counter (cumulative since construction).
    pub fn dropped_count(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }

    /// Current pending ring depth.
    pub fn in_flight(&self) -> usize {
        self.ring.len()
    }

    /// Record a reference-impl divergence.
    pub fn record_reference_divergence(
        &self,
        stage: &str,
        kind: SubMsStageKind,
        reference_kind: &str,
        expected: &str,
        observed: &str,
    ) {
        self.drift
            .record(stage, kind, reference_kind, expected, observed);
    }

    /// Stop the drain worker, drain any remaining samples synchronously,
    /// and return. Idempotent.
    pub fn flush(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let mut slot = self.worker.lock().expect("worker handle poisoned");
        if let Some(handle) = slot.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for OtelObserverAsync {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let mut slot = self.worker.lock().expect("worker handle poisoned");
        if let Some(handle) = slot.take() {
            let _ = handle.join();
        }
    }
}

impl OtelObserverAsyncBuilder {
    /// Override the ring capacity. Must be > 0. Rounded up to a power of two.
    pub fn with_capacity(mut self, capacity: usize) -> Self {
        assert!(capacity > 0, "capacity must be > 0");
        self.capacity = capacity;
        self
    }

    /// Override the drain interval.
    pub fn with_drain_interval(mut self, interval: Duration) -> Self {
        self.drain_interval = interval;
        self
    }

    /// Attach an exemplar reservoir.
    #[cfg(feature = "exemplars")]
    pub fn with_exemplar_reservoir(mut self, reservoir: Arc<ExemplarReservoir>) -> Self {
        self.exemplars = Some(reservoir);
        self
    }

    /// Spawn the drain worker and return the observer.
    pub fn build(self) -> OtelObserverAsync {
        let ring = Arc::new(SpscRing::new(self.capacity));
        let dropped = Arc::new(AtomicU64::new(0));
        let summary_cache = Arc::new(Mutex::new(SummaryCache::default()));
        let shutdown = Arc::new(AtomicBool::new(false));

        let ops_counter = self
            .meter
            .u64_counter(OPS_TOTAL_COUNTER_NAME)
            .with_description("Per-stage operation counter; one increment per on_record call")
            .build();
        let exemplars_kept = self
            .meter
            .u64_counter(EXEMPLARS_KEPT_COUNTER_NAME)
            .with_description("Count of samples retained by an attached exemplar reservoir")
            .build();
        let drift = ReferenceDivergenceRecorder::new(self.meter.clone());

        let ring_for_gauge = Arc::clone(&ring);
        self.meter
            .u64_observable_gauge(IN_FLIGHT_GAUGE_NAME)
            .with_description("Pending samples in the async observer's ring.")
            .with_callback(move |observer| {
                observer.observe(ring_for_gauge.len() as u64, &[]);
            })
            .build();

        let meter = self.meter.clone();
        let interval = self.drain_interval;
        let ring_worker = Arc::clone(&ring);
        let summary_cache_worker = Arc::clone(&summary_cache);
        let shutdown_worker = Arc::clone(&shutdown);
        let dropped_worker = Arc::clone(&dropped);

        #[cfg(feature = "exemplars")]
        let exemplars_for_worker = self.exemplars.clone();
        #[cfg(feature = "exemplars")]
        let exemplars_kept_worker = exemplars_kept.clone();

        let worker = thread::Builder::new()
            .name("subms-otel-drain".into())
            .spawn(move || {
                run_drain(
                    meter,
                    ring_worker,
                    interval,
                    summary_cache_worker,
                    shutdown_worker,
                    dropped_worker,
                    #[cfg(feature = "exemplars")]
                    exemplars_for_worker,
                    #[cfg(feature = "exemplars")]
                    exemplars_kept_worker,
                );
            })
            .expect("subms-otel: failed to spawn drain thread");

        OtelObserverAsync {
            meter: self.meter,
            ring,
            dropped,
            summary_cache,
            shutdown,
            worker: Mutex::new(Some(worker)),
            ops_counter,
            exemplars_kept,
            drift,
            #[cfg(feature = "exemplars")]
            exemplars: self.exemplars,
        }
    }
}

impl SubMsObserver for OtelObserverAsync {
    fn on_record(&self, ctx: &ObservationCtx, ns: u64) {
        let sample = Sample {
            stage: Arc::from(ctx.stage),
            workload: Arc::from(ctx.workload),
            lang: Arc::from(ctx.lang),
            kind: ctx.stage_kind,
            ns,
        };
        let attrs = attributes_from_ctx(ctx);
        self.ops_counter.add(1, &attrs);
        if self.ring.push(sample).is_err() {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
        #[cfg(feature = "exemplars")]
        {
            self.offer_exemplar(ctx, ns, &attrs);
        }
    }

    fn on_summarize(&self, summary: &SubMsBenchSummary) {
        let mut cache = self.summary_cache.lock().expect("summary cache poisoned");
        cache.inputs = summary.inputs.clone();
        cache.meta = summary.meta.clone();
        drop(cache);
        #[cfg(feature = "exemplars")]
        if let Some(reservoir) = self.exemplars.as_ref() {
            reservoir.publish(&self.meter);
        }
    }
}

impl OtelObserverAsync {
    #[cfg(feature = "exemplars")]
    fn offer_exemplar(&self, ctx: &ObservationCtx<'_>, ns: u64, attrs: &[opentelemetry::KeyValue]) {
        let Some(reservoir) = self.exemplars.as_ref() else {
            return;
        };
        if reservoir.offer(ctx, ns) {
            self.exemplars_kept.add(1, attrs);
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_drain(
    meter: Meter,
    ring: Arc<SpscRing>,
    interval: Duration,
    summary_cache: Arc<Mutex<SummaryCache>>,
    shutdown: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    #[cfg(feature = "exemplars")] _exemplars: Option<Arc<ExemplarReservoir>>,
    #[cfg(feature = "exemplars")] _exemplars_kept: Counter<u64>,
) {
    let mut histograms: Vec<((String, SubMsStageKind), Histogram<f64>)> = Vec::new();
    let dropped_counter: Counter<u64> = meter
        .u64_counter(DROPPED_TOTAL_COUNTER_NAME)
        .with_description("Cumulative samples dropped under back-pressure")
        .build();
    let mut last_emitted_dropped: u64 = 0;

    loop {
        while let Some(sample) = ring.pop() {
            let histogram = histogram_lookup(&mut histograms, &meter, &sample.stage, sample.kind);
            let attrs = attrs_for_sample(&sample, &summary_cache);
            histogram.record(sample.ns as f64 / 1e9, &attrs);
        }

        let cur_dropped = dropped.load(Ordering::Relaxed);
        if cur_dropped > last_emitted_dropped {
            dropped_counter.add(cur_dropped - last_emitted_dropped, &[]);
            last_emitted_dropped = cur_dropped;
        }

        if shutdown.load(Ordering::SeqCst) {
            while let Some(sample) = ring.pop() {
                let histogram =
                    histogram_lookup(&mut histograms, &meter, &sample.stage, sample.kind);
                let attrs = attrs_for_sample(&sample, &summary_cache);
                histogram.record(sample.ns as f64 / 1e9, &attrs);
            }
            let cur_dropped = dropped.load(Ordering::Relaxed);
            if cur_dropped > last_emitted_dropped {
                dropped_counter.add(cur_dropped - last_emitted_dropped, &[]);
            }
            return;
        }

        thread::sleep(interval);
    }
}

fn histogram_lookup<'a>(
    cache: &'a mut Vec<((String, SubMsStageKind), Histogram<f64>)>,
    meter: &Meter,
    stage: &str,
    kind: SubMsStageKind,
) -> &'a Histogram<f64> {
    let pos = cache
        .iter()
        .position(|((n, k), _)| n == stage && *k == kind);
    let idx = match pos {
        Some(i) => i,
        None => {
            let histogram = histogram_for_stage(meter, kind);
            cache.push(((stage.to_string(), kind), histogram));
            cache.len() - 1
        }
    };
    &cache[idx].1
}

fn attrs_for_sample(
    sample: &Sample,
    summary_cache: &Arc<Mutex<SummaryCache>>,
) -> Vec<opentelemetry::KeyValue> {
    let ctx = ObservationCtx {
        workload: &sample.workload,
        lang: &sample.lang,
        stage: &sample.stage,
        stage_kind: sample.kind,
    };
    let mut attrs = attributes_from_ctx(&ctx);
    let cache = summary_cache.lock().expect("summary cache poisoned");
    push_meta_attrs(&mut attrs, &cache.inputs, &cache.meta);
    attrs
}
