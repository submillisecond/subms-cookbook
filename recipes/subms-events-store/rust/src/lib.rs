//! `subms-events-store` - in-memory event sourcing on
//! [`subms-events`](https://www.submillisecond.com/cookbook/recipes/subms-events),
//! std-only and zero-dep.
//!
//! Append events to a log, address them by offset, fold the log into a read
//! model (full [`replay`] or an incremental [`Projector`]), and fan appended
//! events to live subscribers. Durability is out of scope - pair with
//! `subms-ts-wal` to persist.
//!
//! ```
//! use subms_events_store::{Event, EventStore, Projector};
//!
//! let mut store = EventStore::new();
//! store.append(Event::builder("order.filled").at("t0").attr("qty", "100").build());
//! store.append(Event::builder("order.filled").at("t1").attr("qty", "50").build());
//!
//! // Incremental read model: catch_up folds only the new tail.
//! let mut filled = Projector::new(0u64);
//! filled.catch_up(&store, |n, _e| *n += 1);
//! assert_eq!(*filled.state(), 2);
//!
//! // Appending more only costs the tail on the next catch_up.
//! store.append(Event::builder("order.filled").at("t2").attr("qty", "25").build());
//! filled.catch_up(&store, |n, _e| *n += 1);
//! assert_eq!(*filled.state(), 3);
//! ```

mod event_store;
mod projection;

#[cfg(feature = "harness")]
pub mod recipe;

pub use event_store::EventStore;
pub use projection::{Projector, replay};

// Re-export the subms-events surface a consumer needs, so they depend on one crate.
pub use subms_events::{DispatchMode, Event, EventBuilder, EventLevel, EventListener, listener};

#[cfg(test)]
#[path = "store_tests.rs"]
mod store_tests;

#[cfg(test)]
#[path = "property_tests.rs"]
mod property_tests;

#[cfg(test)]
#[path = "sample_app_tests.rs"]
mod sample_app_tests;
