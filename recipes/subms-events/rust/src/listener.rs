//! Listeners: the `EventListener` contract plus closure / composite / filter
//! adapters. A listener receives an `&Event`; in async dispatch it runs on the
//! dispatcher thread, in sync dispatch inline on the emitter.

use std::sync::Arc;

use crate::event::Event;

/// Receives events. Keep it quick in sync dispatch; in async dispatch a backlog
/// grows the channel rather than blocking the emitter.
pub trait EventListener: Send + Sync {
    fn on_event(&self, event: &Event);
}

/// Closure adapter.
pub struct FnEventListener<F>(pub F);

impl<F: Fn(&Event) + Send + Sync> EventListener for FnEventListener<F> {
    fn on_event(&self, event: &Event) {
        (self.0)(event)
    }
}

/// Build a listener from a closure: `listener(|e| ...)`.
pub fn listener<F>(f: F) -> Arc<dyn EventListener>
where
    F: Fn(&Event) + Send + Sync + 'static,
{
    Arc::new(FnEventListener(f))
}

/// Fan-out: deliver each event to several listeners in order.
pub struct CompositeListener {
    listeners: Vec<Arc<dyn EventListener>>,
}

impl CompositeListener {
    pub fn new(listeners: Vec<Arc<dyn EventListener>>) -> Self {
        Self { listeners }
    }
    pub fn push(&mut self, listener: Arc<dyn EventListener>) -> &mut Self {
        self.listeners.push(listener);
        self
    }
}

impl EventListener for CompositeListener {
    fn on_event(&self, event: &Event) {
        for l in &self.listeners {
            l.on_event(event);
        }
    }
}

/// Gate: forward to the inner listener only when the predicate passes (filter by
/// topic, level, attribute, ...).
pub struct FilterListener {
    predicate: Box<dyn Fn(&Event) -> bool + Send + Sync>,
    inner: Arc<dyn EventListener>,
}

impl FilterListener {
    pub fn new<P>(predicate: P, inner: Arc<dyn EventListener>) -> Self
    where
        P: Fn(&Event) -> bool + Send + Sync + 'static,
    {
        Self {
            predicate: Box::new(predicate),
            inner,
        }
    }
}

impl EventListener for FilterListener {
    fn on_event(&self, event: &Event) {
        if (self.predicate)(event) {
            self.inner.on_event(event);
        }
    }
}
