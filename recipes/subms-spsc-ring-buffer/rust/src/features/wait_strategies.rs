//! Blocking wrappers around the base wait-free SPSC ring.
//!
//! The base `Producer` / `Consumer` return immediately on full / empty. These
//! wrappers take a `WaitStrategy` and block when the ring isn't ready. Three
//! strategies are provided:
//!
//! - `BusySpin` - tight `spin_loop` hint; lowest wakeup latency, highest CPU.
//! - `YieldStrategy` - calls `thread::yield_now` between retries; lets other
//!   threads run, decent default for over-subscribed cores.
//! - `ParkStrategy` - `thread::park` + producer-side `unpark`; lowest CPU,
//!   adds a syscall and a few microseconds of wakeup latency.
//!
//! All wrappers are wait-free in the base case (slot available immediately).
//! They become blocking ONLY when the ring is full / empty.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, Thread};

use crate::{Consumer, Producer};

/// Strategy for backing off when the ring is full (producer) or empty
/// (consumer). Implementations should be cheap to construct and `Send`able
/// to the owning thread.
pub trait WaitStrategy: Send {
    /// Wait briefly. Called between retries when the ring isn't ready.
    fn wait(&mut self);
    /// Signal the waiter. Default is a no-op; only `ParkStrategy` overrides.
    fn signal(&self) {}
}

/// Tight CPU spin. Use when the producer and consumer are on dedicated cores
/// and latency matters more than CPU usage.
pub struct BusySpin;
impl WaitStrategy for BusySpin {
    fn wait(&mut self) {
        std::hint::spin_loop();
    }
}

/// `thread::yield_now` between retries. Good default when the threads share
/// cores with other work.
pub struct YieldStrategy;
impl WaitStrategy for YieldStrategy {
    fn wait(&mut self) {
        thread::yield_now();
    }
}

/// Park the calling thread; the opposite-end wrapper `unpark`s it on progress.
/// Lowest CPU; adds wakeup latency.
///
/// Both ends share a `Parker` instance via `Arc` so the producer can wake the
/// consumer and vice versa.
pub struct ParkStrategy {
    parker: Arc<Parker>,
    is_producer: bool,
}

impl ParkStrategy {
    /// Build a matched `(producer_strategy, consumer_strategy)` pair that
    /// share a parking state.
    pub fn pair() -> (Self, Self) {
        let p = Arc::new(Parker::new());
        (
            Self {
                parker: p.clone(),
                is_producer: true,
            },
            Self {
                parker: p,
                is_producer: false,
            },
        )
    }
}

impl WaitStrategy for ParkStrategy {
    fn wait(&mut self) {
        // Park the calling thread, recording it as the side that's blocked.
        // The opposite end's `signal()` will unpark it.
        if self.is_producer {
            self.parker.park_producer();
        } else {
            self.parker.park_consumer();
        }
    }

    fn signal(&self) {
        // Wake the OPPOSITE side (the producer wakes the consumer and vice versa).
        if self.is_producer {
            self.parker.unpark_consumer();
        } else {
            self.parker.unpark_producer();
        }
    }
}

/// Internal: cross-end park state. Stores the parked Thread handle for each
/// side and an `unparked` flag to defeat lost-wakeup races.
struct Parker {
    producer: parking_lot::Mutex<Option<Thread>>,
    consumer: parking_lot::Mutex<Option<Thread>>,
    producer_unparked: AtomicBool,
    consumer_unparked: AtomicBool,
}

impl Parker {
    fn new() -> Self {
        Self {
            producer: parking_lot::Mutex::new(None),
            consumer: parking_lot::Mutex::new(None),
            producer_unparked: AtomicBool::new(false),
            consumer_unparked: AtomicBool::new(false),
        }
    }

    fn park_producer(&self) {
        // Fast path: prior unpark already pending - consume it without sleeping.
        if self.producer_unparked.swap(false, Ordering::Acquire) {
            return;
        }
        {
            let mut slot = self.producer.lock();
            *slot = Some(thread::current());
        }
        // Re-check after registering: the unpark might have raced with our
        // registration; if it did, swap returns true and we skip the park.
        if self.producer_unparked.swap(false, Ordering::Acquire) {
            return;
        }
        thread::park();
        // Clear any residual flag set during the park.
        self.producer_unparked.store(false, Ordering::Release);
    }

    fn park_consumer(&self) {
        if self.consumer_unparked.swap(false, Ordering::Acquire) {
            return;
        }
        {
            let mut slot = self.consumer.lock();
            *slot = Some(thread::current());
        }
        if self.consumer_unparked.swap(false, Ordering::Acquire) {
            return;
        }
        thread::park();
        self.consumer_unparked.store(false, Ordering::Release);
    }

    fn unpark_producer(&self) {
        self.producer_unparked.store(true, Ordering::Release);
        if let Some(t) = self.producer.lock().take() {
            t.unpark();
        }
    }

    fn unpark_consumer(&self) {
        self.consumer_unparked.store(true, Ordering::Release);
        if let Some(t) = self.consumer.lock().take() {
            t.unpark();
        }
    }
}

// Mini lock impl - no external dep, mirrors a basic mutex. We need it for
// safe handoff of Thread handles between producer and consumer.
mod parking_lot {
    use std::cell::UnsafeCell;
    use std::sync::atomic::{AtomicBool, Ordering};

    pub struct Mutex<T> {
        locked: AtomicBool,
        inner: UnsafeCell<T>,
    }

    unsafe impl<T: Send> Sync for Mutex<T> {}
    unsafe impl<T: Send> Send for Mutex<T> {}

    pub struct Guard<'a, T> {
        m: &'a Mutex<T>,
    }

    impl<T> Mutex<T> {
        pub fn new(value: T) -> Self {
            Self {
                locked: AtomicBool::new(false),
                inner: UnsafeCell::new(value),
            }
        }

        pub fn lock(&self) -> Guard<'_, T> {
            while self
                .locked
                .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
                .is_err()
            {
                std::hint::spin_loop();
            }
            Guard { m: self }
        }
    }

    impl<T> std::ops::Deref for Guard<'_, T> {
        type Target = T;
        fn deref(&self) -> &T {
            unsafe { &*self.m.inner.get() }
        }
    }

    impl<T> std::ops::DerefMut for Guard<'_, T> {
        fn deref_mut(&mut self) -> &mut T {
            unsafe { &mut *self.m.inner.get() }
        }
    }

    impl<T> Drop for Guard<'_, T> {
        fn drop(&mut self) {
            self.m.locked.store(false, Ordering::Release);
        }
    }
}

/// Blocking producer: `push(value)` waits for a free slot using the strategy.
pub struct BlockingSpscProducer<T, S: WaitStrategy> {
    inner: Producer<T>,
    strategy: S,
}

impl<T, S: WaitStrategy> BlockingSpscProducer<T, S> {
    pub fn new(producer: Producer<T>, strategy: S) -> Self {
        Self {
            inner: producer,
            strategy,
        }
    }

    /// Block until the value can be pushed.
    pub fn push(&mut self, mut value: T) {
        loop {
            match self.inner.try_push(value) {
                Ok(()) => {
                    // Wake the consumer if it was parked.
                    self.strategy.signal();
                    return;
                }
                Err(returned) => {
                    value = returned;
                    self.strategy.wait();
                }
            }
        }
    }

    /// Non-blocking try_push; pass-through to the underlying ring.
    pub fn try_push(&mut self, value: T) -> Result<(), T> {
        let r = self.inner.try_push(value);
        if r.is_ok() {
            self.strategy.signal();
        }
        r
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

/// Blocking consumer: `pop()` waits for an item using the strategy.
pub struct BlockingSpscConsumer<T, S: WaitStrategy> {
    inner: Consumer<T>,
    strategy: S,
}

impl<T, S: WaitStrategy> BlockingSpscConsumer<T, S> {
    pub fn new(consumer: Consumer<T>, strategy: S) -> Self {
        Self {
            inner: consumer,
            strategy,
        }
    }

    /// Block until an item is available.
    pub fn pop(&mut self) -> T {
        loop {
            if let Some(v) = self.inner.try_pop() {
                // Wake the producer if it was parked on a full ring.
                self.strategy.signal();
                return v;
            }
            self.strategy.wait();
        }
    }

    pub fn try_pop(&mut self) -> Option<T> {
        let v = self.inner.try_pop();
        if v.is_some() {
            self.strategy.signal();
        }
        v
    }

    pub fn capacity(&self) -> usize {
        self.inner.capacity()
    }
}

#[cfg(test)]
mod tests {
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::SpscRingBuffer;

    #[test]
    fn busy_spin_pushes_and_pops_in_a_single_thread() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let mut p = BlockingSpscProducer::new(tx, BusySpin);
        let mut c = BlockingSpscConsumer::new(rx, BusySpin);
        p.push(7);
        assert_eq!(c.pop(), 7);
    }

    #[test]
    fn yield_strategy_handles_full_then_drains() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let mut p = BlockingSpscProducer::new(tx, YieldStrategy);
        let mut c = BlockingSpscConsumer::new(rx, YieldStrategy);

        let consumer = thread::spawn(move || {
            let mut v = Vec::new();
            for _ in 0..20 {
                v.push(c.pop());
            }
            v
        });
        for i in 0..20u32 {
            p.push(i);
        }
        let received = consumer.join().unwrap();
        for (i, v) in received.iter().enumerate() {
            assert_eq!(*v, i as u32);
        }
    }

    #[test]
    fn park_strategy_wakes_blocked_consumer() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let (p_strat, c_strat) = ParkStrategy::pair();
        let mut p = BlockingSpscProducer::new(tx, p_strat);
        let mut c = BlockingSpscConsumer::new(rx, c_strat);

        let started = Instant::now();
        let consumer = thread::spawn(move || c.pop());

        // Make sure the consumer is parked before we push.
        thread::sleep(Duration::from_millis(20));
        p.push(42);

        let v = consumer.join().unwrap();
        assert_eq!(v, 42);
        // Bounded so a missed unpark fails fast rather than hanging.
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "consumer didn't wake within 2s"
        );
    }

    #[test]
    fn park_strategy_wakes_blocked_producer() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(2);
        let (p_strat, c_strat) = ParkStrategy::pair();
        let mut p = BlockingSpscProducer::new(tx, p_strat);
        let mut c = BlockingSpscConsumer::new(rx, c_strat);

        // Fill the ring before spawning so the producer thread will block.
        p.push(1);
        p.push(2);

        let started = Instant::now();
        let producer = thread::spawn(move || {
            p.push(3);
            p.push(4);
        });

        thread::sleep(Duration::from_millis(20));
        assert_eq!(c.pop(), 1);
        assert_eq!(c.pop(), 2);
        producer.join().unwrap();
        assert_eq!(c.pop(), 3);
        assert_eq!(c.pop(), 4);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "producer didn't unblock within 2s"
        );
    }

    #[test]
    fn park_strategy_handles_high_throughput_round_trip() {
        let (tx, rx) = SpscRingBuffer::with_capacity::<u64>(64);
        let (p_strat, c_strat) = ParkStrategy::pair();
        let mut p = BlockingSpscProducer::new(tx, p_strat);
        let mut c = BlockingSpscConsumer::new(rx, c_strat);

        let n = 50_000u64;
        let producer = thread::spawn(move || {
            for i in 0..n {
                p.push(i);
            }
        });
        let consumer = thread::spawn(move || {
            for i in 0..n {
                assert_eq!(c.pop(), i);
            }
        });
        producer.join().unwrap();
        consumer.join().unwrap();
    }

    #[test]
    fn try_push_succeeds_without_blocking_when_slot_free() {
        let (tx, _rx) = SpscRingBuffer::with_capacity::<u32>(4);
        let mut p = BlockingSpscProducer::new(tx, BusySpin);
        assert!(p.try_push(1).is_ok());
        assert!(p.try_push(2).is_ok());
    }
}
