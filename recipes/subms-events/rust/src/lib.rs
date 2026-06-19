//! `subms-events` - a low-latency in-process event system, std-only and zero-dep.
//!
//! A structured [`Event`] (+ fluent [`EventBuilder`]), an [`EventDispatcher`] that
//! delivers either inline (`DispatchMode::Sync`, no thread) or off-thread
//! (`DispatchMode::Async`, non-blocking emit), composable [`EventListener`]s
//! (closure / [`CompositeListener`] / [`FilterListener`]), and an [`EventBridge`]
//! sink interface that adapters (e.g. `subms-otel`) implement to forward events
//! to an external system.
//!
//! ```
//! use std::sync::{Arc, Mutex};
//! use subms_events::{Event, EventLevel, EventDispatcher, listener};
//!
//! let seen = Arc::new(Mutex::new(Vec::new()));
//! let sink = Arc::clone(&seen);
//!
//! let mut bus = EventDispatcher::sync(); // inline, no background thread
//! bus.add_listener(listener(move |e: &Event| sink.lock().unwrap().push(e.topic.clone())));
//!
//! bus.emit(Event::transition("svc.status", EventLevel::Error, "db", "UP", "DOWN"));
//! assert_eq!(seen.lock().unwrap().as_slice(), ["svc.status"]);
//! ```

mod bridge;
mod dispatcher;
mod event;
mod listener;

#[cfg(feature = "harness")]
pub mod recipe;

pub use bridge::{BridgeListener, EventBridge};
pub use dispatcher::{DispatchMode, EmitHandle, EventDispatcher, OverflowPolicy};
pub use event::{Event, EventBuilder, EventLevel};
pub use listener::{CompositeListener, EventListener, FilterListener, FnEventListener, listener};
