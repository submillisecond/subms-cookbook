//! The dispatcher. `emit` is the hot path: in `Sync` mode it calls listeners
//! inline (no thread, no allocation beyond the event); in `Async` mode it hands
//! the event to a dedicated dispatcher thread over a queue, so a slow listener
//! never blocks the emitter. The async queue is unbounded by default; use
//! `bounded` to cap it and pick an [`OverflowPolicy`].

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::{self, JoinHandle};

use crate::bridge::{BridgeListener, EventBridge};
use crate::event::Event;
use crate::listener::EventListener;

/// How listeners are invoked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DispatchMode {
    /// Inline on the emitting thread - no dispatcher thread, no queue. For
    /// low-latency / no-extra-thread deployments. Keep listeners cheap.
    Sync,
    /// On a dedicated dispatcher thread (default). A slow listener can't stall
    /// the emitter.
    Async,
}

/// What a bounded async dispatcher does when the queue is full.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// Block the emitter until space frees (back-pressure to the producer).
    Block,
    /// Drop the incoming event (keep the backlog).
    DropNewest,
    /// Evict the oldest queued event to make room (keep the freshest).
    DropOldest,
}

struct Inner {
    mode: DispatchMode,
    listeners: Mutex<Vec<Arc<dyn EventListener>>>,
    queue: Mutex<VecDeque<Event>>,
    not_empty: Condvar,
    not_full: Condvar,
    capacity: Option<usize>, // None = unbounded
    policy: OverflowPolicy,
    running: AtomicBool,
    stop: AtomicBool,
    dropped: AtomicU64,
}

impl Inner {
    fn dispatch(&self, event: Event) {
        match self.mode {
            DispatchMode::Sync => {
                let listeners = self.listeners.lock().unwrap().clone();
                for l in &listeners {
                    l.on_event(&event);
                }
            }
            DispatchMode::Async => self.enqueue(event),
        }
    }

    fn enqueue(&self, event: Event) {
        if !self.running.load(Ordering::Acquire) {
            return; // no consumer thread yet (or stopped): drop, like a None sender
        }
        let mut q = self.queue.lock().unwrap();
        if let Some(cap) = self.capacity {
            if q.len() >= cap {
                match self.policy {
                    OverflowPolicy::DropNewest => {
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                        return;
                    }
                    OverflowPolicy::DropOldest => {
                        q.pop_front();
                        self.dropped.fetch_add(1, Ordering::Relaxed);
                    }
                    OverflowPolicy::Block => {
                        while q.len() >= cap && !self.stop.load(Ordering::Acquire) {
                            q = self.not_full.wait(q).unwrap();
                        }
                        if self.stop.load(Ordering::Acquire) {
                            return;
                        }
                    }
                }
            }
        }
        q.push_back(event);
        drop(q);
        self.not_empty.notify_one();
    }
}

/// A cheap, cloneable emitter. Hand it to many producers (or a background thread)
/// that need to `emit` without owning the dispatcher; the owner keeps the
/// dispatcher for `add_listener` / `stop`.
#[derive(Clone)]
pub struct EmitHandle {
    inner: Arc<Inner>,
}

impl EmitHandle {
    pub fn emit(&self, event: Event) {
        self.inner.dispatch(event);
    }
    pub fn mode(&self) -> DispatchMode {
        self.inner.mode
    }
}

/// An in-process event dispatcher. Register listeners / bridges, then `emit`.
pub struct EventDispatcher {
    inner: Arc<Inner>,
    handle: Option<JoinHandle<()>>,
}

impl EventDispatcher {
    fn build(mode: DispatchMode, capacity: Option<usize>, policy: OverflowPolicy) -> Self {
        Self {
            inner: Arc::new(Inner {
                mode,
                listeners: Mutex::new(Vec::new()),
                queue: Mutex::new(VecDeque::new()),
                not_empty: Condvar::new(),
                not_full: Condvar::new(),
                capacity,
                policy,
                running: AtomicBool::new(false),
                stop: AtomicBool::new(false),
                dropped: AtomicU64::new(0),
            }),
            handle: None,
        }
    }

    pub fn new(mode: DispatchMode) -> Self {
        Self::build(mode, None, OverflowPolicy::Block)
    }

    /// Inline, no-thread dispatcher (the low-latency default for tight loops).
    pub fn sync() -> Self {
        Self::new(DispatchMode::Sync)
    }

    /// Off-thread dispatcher with an unbounded queue.
    pub fn asynchronous() -> Self {
        Self::new(DispatchMode::Async)
    }

    /// Off-thread dispatcher with a bounded queue + overflow policy. Prefer this
    /// in production so a slow listener can't grow the queue without limit.
    pub fn bounded(capacity: usize, policy: OverflowPolicy) -> Self {
        Self::build(DispatchMode::Async, Some(capacity.max(1)), policy)
    }

    pub fn mode(&self) -> DispatchMode {
        self.inner.mode
    }

    /// Count of events dropped under `DropNewest` / `DropOldest`.
    pub fn dropped(&self) -> u64 {
        self.inner.dropped.load(Ordering::Relaxed)
    }

    pub fn listener_count(&self) -> usize {
        self.inner.listeners.lock().unwrap().len()
    }

    /// Register a listener. In async mode this starts the dispatcher thread on
    /// first use.
    pub fn add_listener(&mut self, listener: Arc<dyn EventListener>) -> &mut Self {
        self.inner.listeners.lock().unwrap().push(listener);
        if self.inner.mode == DispatchMode::Async {
            self.ensure_thread();
        }
        self
    }

    /// Register an external bridge (wrapped as a listener).
    pub fn add_bridge(&mut self, bridge: Arc<dyn EventBridge>) -> &mut Self {
        self.add_listener(BridgeListener::new(bridge))
    }

    fn ensure_thread(&mut self) {
        if self.handle.is_some() {
            return;
        }
        let inner = Arc::clone(&self.inner);
        inner.running.store(true, Ordering::Release);
        let handle = thread::Builder::new()
            .name("subms-events-dispatch".to_string())
            .spawn(move || run_consumer(inner))
            .expect("spawn dispatcher");
        self.handle = Some(handle);
    }

    /// Emit an event. Sync: listeners run inline. Async: enqueued for the
    /// dispatcher thread. With no listeners this is a cheap no-op.
    pub fn emit(&self, event: Event) {
        self.inner.dispatch(event);
    }

    /// A cloneable emit handle for producers that don't own the dispatcher.
    pub fn handle(&self) -> EmitHandle {
        EmitHandle {
            inner: Arc::clone(&self.inner),
        }
    }

    /// Stop the dispatcher thread (async only) and join it.
    pub fn stop(&mut self) {
        if let Some(h) = self.handle.take() {
            self.inner.stop.store(true, Ordering::Release);
            self.inner.running.store(false, Ordering::Release);
            self.inner.not_empty.notify_all();
            self.inner.not_full.notify_all();
            let _ = h.join();
        }
    }
}

impl Drop for EventDispatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// The dispatcher thread body: park on the queue, drain, fan out to listeners,
/// exit when `stop` is set and the queue is empty.
fn run_consumer(inner: Arc<Inner>) {
    loop {
        let event = {
            let mut q = inner.queue.lock().unwrap();
            while q.is_empty() && !inner.stop.load(Ordering::Acquire) {
                q = inner.not_empty.wait(q).unwrap();
            }
            if q.is_empty() && inner.stop.load(Ordering::Acquire) {
                return;
            }
            q.pop_front().unwrap()
        };
        inner.not_full.notify_one();
        let listeners = inner.listeners.lock().unwrap().clone();
        for l in &listeners {
            l.on_event(&event);
        }
    }
}
