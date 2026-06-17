//! `subms-ts-cdc` - a change-data-capture / subscribe layer over
//! [`subms_ts::TsCollection`]. [`TsObservableCollection`] is a decorator: it
//! owns an inner collection and mirrors every mutation onto one wait-free SPSC
//! ring per subscriber. A subscriber drains its [`TsSubscription`] at its own
//! pace; a slow consumer drops events rather than back-pressuring the writer.
//!
//! The wrapper is standalone - it does not touch the `subms-ts` core. The
//! mutating surface (`register` / `push` / `delete_at` / `delete_range` /
//! `deregister`) is mirrored 1:1; reads delegate to the inner collection via
//! [`TsObservableCollection::collection`].
//!
//! ```
//! use subms_ts::TsSeriesMetadata;
//! use subms_ts_cdc::{TsObservableCollection, TsChangeEvent};
//!
//! let mut obs = TsObservableCollection::<f64>::new();
//! let mut sub = obs.subscribe(64);
//! let id = obs.register(TsSeriesMetadata::new(1, "px")).unwrap();
//! obs.push(id, 1_000, 42.5).unwrap();
//!
//! assert_eq!(
//!     sub.try_recv(),
//!     Some(TsChangeEvent::Push { series_id: id, ts: 1_000, value: 42.5 }),
//! );
//! ```

use subms_spsc_ring_buffer::{Consumer, Producer, SpscRingBuffer};
use subms_ts::{TsCollection, TsCollectionError, TsPoint, TsSeries, TsSeriesMetadata, TsValueKind};

/// A single mutation observed on the collection, published in commit order.
///
/// One variant per mirrored mutator. `Push` carries the appended sample;
/// `DeleteAt` / `DeleteRange` carry the key span removed; `Deregister`
/// announces a whole series leaving the registry.
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum TsChangeEvent<T> {
    Push { series_id: u64, ts: i64, value: T },
    DeleteAt { series_id: u64, ts: i64 },
    DeleteRange { series_id: u64, lo: i64, hi: i64 },
    Deregister { series_id: u64 },
}

/// The read end of one subscriber's ring. Owns the [`Consumer`] half of an
/// SPSC pair; the matching producer lives in the [`TsObservableCollection`].
/// Move it to the consuming thread - SPSC means exactly one reader.
pub struct TsSubscription<T> {
    rx: Consumer<TsChangeEvent<T>>,
}

impl<T: Copy> TsSubscription<T> {
    /// Pop the next event, or `None` if the ring is currently empty. Wait-free.
    pub fn try_recv(&mut self) -> Option<TsChangeEvent<T>> {
        self.rx.try_pop()
    }

    /// Drain every currently-buffered event in FIFO order. Stops at the first
    /// empty pop - events published after the drain starts are simply seen on
    /// the next call.
    pub fn drain(&mut self) -> Vec<TsChangeEvent<T>> {
        let mut out = Vec::new();
        while let Some(ev) = self.rx.try_pop() {
            out.push(ev);
        }
        out
    }
}

/// A [`TsCollection`] that publishes its mutations to subscribers.
///
/// Each [`subscribe`](Self::subscribe) hands back a [`TsSubscription`] backed
/// by its own ring; the collection holds the producer end of every ring. A
/// mutation runs against the inner collection first, then fans the matching
/// [`TsChangeEvent`] out to every ring. A full ring is skipped and counted in
/// [`dropped_events`](Self::dropped_events) - the mutation still succeeds, so a
/// stalled reader never blocks the writer.
///
/// With zero subscribers the publish path is an empty-slice walk: no events are
/// constructed and `push` runs at inner-collection speed.
pub struct TsObservableCollection<T> {
    inner: TsCollection<T>,
    subscribers: Vec<Producer<TsChangeEvent<T>>>,
    dropped: u64,
}

impl<T: Copy> Default for TsObservableCollection<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Copy> TsObservableCollection<T> {
    pub fn new() -> Self {
        Self {
            inner: TsCollection::new(),
            subscribers: Vec::new(),
            dropped: 0,
        }
    }

    /// Register a new subscriber. `capacity` is the requested ring depth
    /// (rounded up to a power of two by the ring). Returns the read handle;
    /// the write end is retained internally.
    pub fn subscribe(&mut self, capacity: usize) -> TsSubscription<T> {
        let (tx, rx) = SpscRingBuffer::with_capacity::<TsChangeEvent<T>>(capacity);
        self.subscribers.push(tx);
        TsSubscription { rx }
    }

    /// Number of subscribers with a live ring.
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }

    /// Total events dropped across all rings because the target ring was full
    /// at publish time. Monotonic for the lifetime of the collection.
    pub fn dropped_events(&self) -> u64 {
        self.dropped
    }

    /// Borrow the inner collection for reads (`get`, `by_name`, `len`, ...).
    pub fn collection(&self) -> &TsCollection<T> {
        &self.inner
    }

    fn publish(&mut self, event: TsChangeEvent<T>) {
        if self.subscribers.is_empty() {
            return;
        }
        let mut dropped = 0u64;
        for tx in &mut self.subscribers {
            if tx.try_push(event).is_err() {
                dropped += 1;
            }
        }
        self.dropped += dropped;
    }

    // ---------- mirrored mutating surface ----------

    /// Register an empty series. No event is published - registration is not a
    /// data change, and the consumer learns of the series on its first `Push`.
    pub fn register(&mut self, meta: TsSeriesMetadata) -> Result<u64, TsCollectionError> {
        self.inner.register(meta)
    }

    /// Append a point, then publish [`TsChangeEvent::Push`]. The event fires
    /// only when the inner push succeeds.
    pub fn push(&mut self, id: u64, ts: i64, value: T) -> Result<(), TsCollectionError>
    where
        T: TsValueKind,
    {
        self.inner.push(id, ts, value)?;
        self.publish(TsChangeEvent::Push {
            series_id: id,
            ts,
            value,
        });
        Ok(())
    }

    /// Delete the point at `ts`, then publish [`TsChangeEvent::DeleteAt`] iff a
    /// point was actually removed.
    pub fn delete_at(&mut self, id: u64, ts: i64) -> Option<TsPoint<T>> {
        let removed = self.inner.delete_at(id, ts);
        if removed.is_some() {
            self.publish(TsChangeEvent::DeleteAt { series_id: id, ts });
        }
        removed
    }

    /// Delete the `[lo, hi]` range, then publish [`TsChangeEvent::DeleteRange`]
    /// iff at least one point was removed. Returns the removed count.
    pub fn delete_range(&mut self, id: u64, lo: i64, hi: i64) -> usize {
        let n = self.inner.delete_range(id, lo, hi);
        if n > 0 {
            self.publish(TsChangeEvent::DeleteRange {
                series_id: id,
                lo,
                hi,
            });
        }
        n
    }

    /// Deregister a series, then publish [`TsChangeEvent::Deregister`] iff the
    /// series existed. Returns the removed series.
    pub fn deregister(&mut self, id: u64) -> Option<TsSeries<T>> {
        let removed = self.inner.deregister(id);
        if removed.is_some() {
            self.publish(TsChangeEvent::Deregister { series_id: id });
        }
        removed
    }
}

#[cfg(feature = "harness")]
pub mod recipe;
