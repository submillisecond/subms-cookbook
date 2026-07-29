//! Pins the behaviour the `sample_app` example demonstrates: one emit fans out to
//! every registered listener, a `FilterListener` gates by topic, and an
//! `EventBridge` sink sees the deterministic wire JSON of every event.

use std::sync::{Arc, Mutex};

use subms_events::{Event, EventBridge, EventDispatcher, EventLevel, FilterListener, listener};

struct AuditLog {
    lines: Arc<Mutex<Vec<String>>>,
}

impl EventBridge for AuditLog {
    fn name(&self) -> &str {
        "audit"
    }
    fn forward(&self, event: &Event) {
        self.lines.lock().unwrap().push(event.to_json());
    }
}

#[test]
fn order_bus_fans_out_to_every_handler() {
    let filled_notional = Arc::new(Mutex::new(0u64));
    let ledger = Arc::new(Mutex::new(Vec::<String>::new()));
    let alerts = Arc::new(Mutex::new(Vec::<String>::new()));
    let audit = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut bus = EventDispatcher::sync();

    let risk_sink = Arc::clone(&filled_notional);
    bus.add_listener(listener(move |e: &Event| {
        if e.topic == "order.filled" {
            if let Some(qty) = e.attr("notional").and_then(|n| n.parse::<u64>().ok()) {
                *risk_sink.lock().unwrap() += qty;
            }
        }
    }));

    let ledger_sink = Arc::clone(&ledger);
    bus.add_listener(listener(move |e: &Event| {
        ledger_sink.lock().unwrap().push(e.topic.clone());
    }));

    let alert_sink = Arc::clone(&alerts);
    bus.add_listener(Arc::new(FilterListener::new(
        |e: &Event| e.topic == "order.filled",
        listener(move |e: &Event| {
            alert_sink
                .lock()
                .unwrap()
                .push(e.attr("id").unwrap_or("?").to_string());
        }),
    )));

    bus.add_bridge(Arc::new(AuditLog {
        lines: Arc::clone(&audit),
    }));

    bus.emit(Event::builder("order.accepted").attr("id", "A-1").build());
    bus.emit(
        Event::builder("order.filled")
            .attr("id", "A-1")
            .attr("notional", "25000")
            .build(),
    );
    bus.emit(
        Event::builder("order.filled")
            .attr("id", "B-7")
            .attr("notional", "40000")
            .build(),
    );
    bus.emit(Event::transition(
        "order.cancelled",
        EventLevel::Warn,
        "C-3",
        "WORKING",
        "CANCELLED",
    ));

    assert_eq!(
        *filled_notional.lock().unwrap(),
        65_000,
        "risk summed both fills"
    );
    assert_eq!(ledger.lock().unwrap().len(), 4, "ledger saw every event");
    assert_eq!(
        alerts.lock().unwrap().as_slice(),
        ["A-1", "B-7"],
        "only fills alerted, in order"
    );
    assert_eq!(audit.lock().unwrap().len(), 4, "bridge saw every event");
}

#[test]
fn audit_bridge_captures_wire_json() {
    let audit = Arc::new(Mutex::new(Vec::<String>::new()));
    let mut bus = EventDispatcher::sync();
    bus.add_bridge(Arc::new(AuditLog {
        lines: Arc::clone(&audit),
    }));

    bus.emit(
        Event::builder("order.filled")
            .attr("id", "A-1")
            .attr("notional", "25000")
            .build(),
    );

    let lines = audit.lock().unwrap();
    assert_eq!(
        lines[0],
        r#"{"topic":"order.filled","level":"INFO","at":"","attributes":{"id":"A-1","notional":"25000"}}"#
    );
}
