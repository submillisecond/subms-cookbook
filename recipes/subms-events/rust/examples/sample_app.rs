//! Sample app: a tour of `subms-events` on a trading-domain event bus. One
//! sync dispatcher fans each order-lifecycle event out to a risk handler, a
//! ledger, and a notifications gate, with an `EventBridge` audit sink capturing
//! the wire JSON. Run: `cargo run --example sample_app`.
//!
//! `subms-events` ships no optional Cargo features, so this base scenario is the
//! whole tour: structured events, multi-handler dispatch, a filtered listener,
//! and a bridge.

use std::sync::{Arc, Mutex};

use subms_events::{Event, EventBridge, EventDispatcher, EventLevel, FilterListener, listener};

/// Stands in for a real sink (a file, an OTEL exporter, a message bus): forwards
/// each event's deterministic JSON to an in-memory log.
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

fn main() {
    println!("== trading order bus: one emit, four handlers ==");

    let filled_notional = Arc::new(Mutex::new(0u64));
    let ledger = Arc::new(Mutex::new(Vec::<String>::new()));
    let alerts = Arc::new(Mutex::new(Vec::<String>::new()));
    let audit = Arc::new(Mutex::new(Vec::<String>::new()));

    let mut bus = EventDispatcher::sync(); // inline dispatch, no background thread

    // Risk: accumulate filled notional so a desk limit can be checked inline.
    let risk_sink = Arc::clone(&filled_notional);
    bus.add_listener(listener(move |e: &Event| {
        if e.topic == "order.filled" {
            if let Some(qty) = e.attr("notional").and_then(|n| n.parse::<u64>().ok()) {
                *risk_sink.lock().unwrap() += qty;
            }
        }
    }));

    // Ledger: record every event by topic.
    let ledger_sink = Arc::clone(&ledger);
    bus.add_listener(listener(move |e: &Event| {
        ledger_sink.lock().unwrap().push(e.topic.clone());
    }));

    // Notifications: only fills reach the trader, via a predicate gate.
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

    // Audit: capture the wire JSON of everything that flows.
    bus.add_bridge(Arc::new(AuditLog {
        lines: Arc::clone(&audit),
    }));

    let events = [
        Event::builder("order.accepted")
            .level(EventLevel::Info)
            .attr("id", "A-1")
            .attr("symbol", "AAPL")
            .build(),
        Event::builder("order.filled")
            .level(EventLevel::Info)
            .attr("id", "A-1")
            .attr("symbol", "AAPL")
            .attr("notional", "25000")
            .build(),
        Event::builder("order.filled")
            .level(EventLevel::Info)
            .attr("id", "B-7")
            .attr("symbol", "MSFT")
            .attr("notional", "40000")
            .build(),
        Event::transition(
            "order.cancelled",
            EventLevel::Warn,
            "C-3",
            "WORKING",
            "CANCELLED",
        ),
    ];
    for e in events {
        println!("  emit {}", e.topic);
        bus.emit(e);
    }

    let notional = *filled_notional.lock().unwrap();
    println!("  risk: filled notional = {notional}");
    println!("  ledger: {:?}", ledger.lock().unwrap());
    println!("  alerts (fills only): {:?}", alerts.lock().unwrap());
    println!("  audit lines captured: {}", audit.lock().unwrap().len());

    assert_eq!(notional, 65_000, "risk summed both fills");
    assert_eq!(ledger.lock().unwrap().len(), 4, "ledger saw every event");
    assert_eq!(
        alerts.lock().unwrap().as_slice(),
        ["A-1", "B-7"],
        "only fills alerted, in order"
    );
    assert_eq!(
        audit.lock().unwrap().len(),
        4,
        "audit bridge saw every event"
    );
}
