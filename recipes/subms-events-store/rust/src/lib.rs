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
//! use subms_events_store::{EventStore, Projector};
//! use subms_events_store::{Event, EventLevel};
//!
//! let mut store = EventStore::new();
//! store.append(Event::builder("user.created").at("t0").attr("id", "7").build());
//! store.append(Event::builder("user.renamed").at("t1").attr("name", "ko").build());
//!
//! // Incremental projection: count events seen.
//! let mut count = Projector::new(0u64);
//! count.catch_up(&store, |n, _e| *n += 1);
//! assert_eq!(*count.state(), 2);
//!
//! // Appending more only costs the tail on the next catch_up.
//! store.append(Event::builder("user.deleted").at("t2").build());
//! count.catch_up(&store, |n, _e| *n += 1);
//! assert_eq!(*count.state(), 3);
//! let _ = EventLevel::Info;
//! ```

mod event_store;
mod projection;

#[cfg(feature = "harness")]
pub mod recipe;

pub use event_store::EventStore;
pub use projection::{Projector, replay};

// Re-export the subms-events surface a consumer needs, so they depend on one crate.
pub use subms_events::{DispatchMode, Event, EventBuilder, EventLevel, EventListener, listener};
