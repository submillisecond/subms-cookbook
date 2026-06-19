//! Zero-dep demo: emit a couple of events through a sync dispatcher with a
//! filtered listener. Run: `cargo run --example demo`.

use std::sync::Arc;

use subms_events::{Event, EventDispatcher, EventLevel, FilterListener, listener};

fn main() {
    let mut bus = EventDispatcher::sync();

    let printer =
        listener(|e: &Event| println!("[{}] {} {}", e.level.as_str(), e.topic, e.to_json()));

    // Only forward WARN and above.
    bus.add_listener(Arc::new(FilterListener::new(
        |e: &Event| matches!(e.level, EventLevel::Warn | EventLevel::Error),
        printer,
    )));

    bus.emit(
        Event::builder("cache.evict")
            .level(EventLevel::Info)
            .attr("keys", "128")
            .build(),
    );
    bus.emit(Event::transition(
        "svc.status",
        EventLevel::Error,
        "db",
        "UP",
        "DOWN",
    ));
    bus.emit(Event::transition(
        "svc.status",
        EventLevel::Info,
        "db",
        "DOWN",
        "UP",
    ));

    println!("(info + recovery events were filtered out)");
}
