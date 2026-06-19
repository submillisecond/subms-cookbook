//! The `EventBridge` sink interface. A bridge forwards events to an external
//! system (OTEL, a log, a message bus). Adapters like `subms-otel` implement it;
//! `BridgeListener` wraps any bridge as an [`EventListener`] so it plugs straight
//! into a dispatcher.

use std::sync::Arc;

use crate::event::Event;
use crate::listener::EventListener;

/// An external event sink. `forward` is called per event; `flush` is an optional
/// hook for buffered bridges (default no-op).
pub trait EventBridge: Send + Sync {
    fn name(&self) -> &str;
    fn forward(&self, event: &Event);
    fn flush(&self) {}
}

/// Adapts an [`EventBridge`] into an [`EventListener`].
pub struct BridgeListener {
    bridge: Arc<dyn EventBridge>,
}

impl BridgeListener {
    pub fn new(bridge: Arc<dyn EventBridge>) -> Arc<Self> {
        Arc::new(Self { bridge })
    }
    pub fn name(&self) -> &str {
        self.bridge.name()
    }
}

impl EventListener for BridgeListener {
    fn on_event(&self, event: &Event) {
        self.bridge.forward(event);
    }
}
