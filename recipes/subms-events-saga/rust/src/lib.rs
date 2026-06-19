//! `subms-events-saga` - an in-process compensating-step (saga) executor on
//! [`subms-events`](https://www.submillisecond.com/cookbook/recipes/subms-events),
//! std-only and zero-dep.
//!
//! Define steps with a forward action + a compensation. [`Saga::run`] executes
//! forwards in order; the first forward failure rolls back the completed steps in
//! reverse. Step lifecycle events flow through a `subms-events` dispatcher.
//!
//! Scope is in-process orchestration: durability, distribution, and the steps'
//! own latency are out of scope (pair with `subms-ts-wal` to persist).
//!
//! ```
//! use subms_events_saga::{Saga, Outcome};
//! use std::sync::{Arc, Mutex};
//!
//! let undone = Arc::new(Mutex::new(Vec::new()));
//! let u = undone.clone();
//! let report = Saga::new("checkout")
//!     .step("reserve", || Ok(()), { let u = u.clone(); move || { u.lock().unwrap().push("reserve"); Ok(()) } })
//!     .step("charge", || Err("card declined".into()), || Ok(()))
//!     .run();
//!
//! assert_eq!(report.outcome, Outcome::Compensated);
//! assert_eq!(report.failed_step.as_deref(), Some("charge"));
//! assert_eq!(*undone.lock().unwrap(), ["reserve"]); // reserve was rolled back
//! ```

mod saga;

#[cfg(feature = "harness")]
pub mod recipe;

pub use saga::{Outcome, Saga, SagaReport};

// Re-export the subms-events surface needed to wire step events.
pub use subms_events::{DispatchMode, EmitHandle, Event, EventDispatcher, EventLevel, listener};
