//! The append-only event log. Stores `subms-events` `Event`s, hands back an
//! offset per append, and (optionally) fans each appended event to live
//! subscribers through an `EventDispatcher`.

use std::sync::Arc;

use subms_events::{DispatchMode, Event, EventDispatcher, EventListener};

/// An in-memory, append-only log of events with offset addressing and live
/// subscriptions. Durability is out of scope - pair with `subms-ts-wal` to
/// persist the log.
pub struct EventStore {
    log: Vec<Event>,
    dispatcher: EventDispatcher,
}

impl EventStore {
    /// A store whose subscribers run inline on `append` (sync dispatch).
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            dispatcher: EventDispatcher::sync(),
        }
    }

    /// Choose how subscribers are notified (sync inline / async off-thread).
    pub fn with_dispatch(mode: DispatchMode) -> Self {
        Self {
            log: Vec::new(),
            dispatcher: EventDispatcher::new(mode),
        }
    }

    /// Append an event; returns its 0-based offset. Subscribers are notified.
    pub fn append(&mut self, event: Event) -> u64 {
        let offset = self.log.len() as u64;
        self.log.push(event.clone());
        self.dispatcher.emit(event);
        offset
    }

    pub fn len(&self) -> usize {
        self.log.len()
    }

    pub fn is_empty(&self) -> bool {
        self.log.is_empty()
    }

    /// The event at `offset`, if any.
    pub fn get(&self, offset: u64) -> Option<&Event> {
        self.log.get(offset as usize)
    }

    /// The whole log.
    pub fn events(&self) -> &[Event] {
        &self.log
    }

    /// Events at `offset..` (empty if `offset` is past the end).
    pub fn read_from(&self, offset: u64) -> &[Event] {
        let i = (offset as usize).min(self.log.len());
        &self.log[i..]
    }

    /// Events whose topic equals `topic`, in order.
    pub fn by_topic<'a>(&'a self, topic: &'a str) -> impl Iterator<Item = &'a Event> + 'a {
        self.log.iter().filter(move |e| e.topic == topic)
    }

    /// Register a live subscriber. It receives every event appended after this
    /// call (and, in async mode, on the dispatcher thread).
    pub fn subscribe(&mut self, listener: Arc<dyn EventListener>) {
        self.dispatcher.add_listener(listener);
    }

    /// Serialize the log as a JSON array of events. Deterministic and
    /// byte-equivalent across the language ports.
    pub fn to_json(&self) -> String {
        let mut out = String::from("[");
        for (i, e) in self.log.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str(&e.to_json());
        }
        out.push(']');
        out
    }
}

impl Default for EventStore {
    fn default() -> Self {
        Self::new()
    }
}
