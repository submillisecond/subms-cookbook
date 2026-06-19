//! Projections: fold the log into a read model. `replay` does a full fold;
//! `Projector` remembers its offset and `catch_up` applies only the events
//! appended since last time - so a hot read model is O(new events), not O(log).

use subms_events::Event;

use crate::event_store::EventStore;

/// Full fold over every event in the store.
pub fn replay<S, F: FnMut(&mut S, &Event)>(store: &EventStore, mut state: S, mut apply: F) -> S {
    for e in store.events() {
        apply(&mut state, e);
    }
    state
}

/// An incremental projection: holds the materialized state and the next offset
/// to read. `catch_up` is the sub-ms path - it applies only the tail.
pub struct Projector<S> {
    state: S,
    next: u64,
}

impl<S> Projector<S> {
    pub fn new(initial: S) -> Self {
        Self {
            state: initial,
            next: 0,
        }
    }

    pub fn state(&self) -> &S {
        &self.state
    }

    pub fn into_state(self) -> S {
        self.state
    }

    /// The offset this projector has consumed up to.
    pub fn position(&self) -> u64 {
        self.next
    }

    /// Apply every event appended since the last `catch_up` and advance the
    /// offset. Returns the updated state.
    pub fn catch_up<F: FnMut(&mut S, &Event)>(&mut self, store: &EventStore, mut apply: F) -> &S {
        for e in store.read_from(self.next) {
            apply(&mut self.state, e);
        }
        self.next = store.len() as u64;
        &self.state
    }
}
