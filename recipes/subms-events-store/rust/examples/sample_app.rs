//! Sample app: event-sourcing a trading position with `subms-events-store`.
//! Append the order-lifecycle events for one instrument, then rebuild the
//! aggregate (net shares + cash) two ways - a cold full `replay` and a live
//! `Projector` that only folds the new tail. Run: `cargo run --example sample_app`.
//!
//! The recipe has no optional Cargo features, so this base scenario is the whole
//! tour: append + offsets, replay/rehydrate, incremental catch_up, topic filter,
//! and a live risk subscriber.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use subms_events_store::{Event, EventStore, Projector, listener, replay};

/// The rebuilt read model: net position in shares and realized cash in cents
/// (negative cash means we have paid out to build the position).
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Position {
    shares: i64,
    cash_cents: i64,
}

/// The single fold rule. Only `order.filled` moves the position; submits and
/// cancels are in the log for audit but do not touch the aggregate.
fn apply_fill(pos: &mut Position, e: &Event) {
    if e.topic != "order.filled" {
        return;
    }
    let qty: i64 = e.attr("qty").and_then(|s| s.parse().ok()).unwrap_or(0);
    let price: i64 = e.attr("price").and_then(|s| s.parse().ok()).unwrap_or(0);
    match e.attr("side") {
        Some("buy") => {
            pos.shares += qty;
            pos.cash_cents -= qty * price;
        }
        Some("sell") => {
            pos.shares -= qty;
            pos.cash_cents += qty * price;
        }
        _ => {}
    }
}

fn fill(side: &str, qty: i64, price: i64, at: &str) -> Event {
    Event::builder("order.filled")
        .at(at)
        .attr("side", side)
        .attr("qty", &qty.to_string())
        .attr("price", &price.to_string())
        .build()
}

fn main() {
    println!("== base: event-sourcing an AAPL position ==");
    let mut store = EventStore::new();

    // A live risk desk tails the log as fills land.
    let fills_seen = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&fills_seen);
    store.subscribe(listener(move |e: &Event| {
        if e.topic == "order.filled" {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }));

    // The order lifecycle, appended in the order it happened. Each append hands
    // back the offset the event landed at.
    let lifecycle = [
        Event::builder("order.submitted")
            .at("09:30:00")
            .attr("id", "A1")
            .attr("side", "buy")
            .attr("qty", "100")
            .build(),
        fill("buy", 100, 18_800, "09:30:01"),
        Event::builder("order.submitted")
            .at("09:45:00")
            .attr("id", "A2")
            .attr("side", "buy")
            .attr("qty", "50")
            .build(),
        fill("buy", 50, 19_010, "09:45:02"),
        Event::builder("order.canceled")
            .at("10:00:00")
            .attr("id", "A3")
            .build(),
    ];
    for e in lifecycle {
        let offset = store.append(e);
        println!(
            "  offset {offset:>2} <- {}",
            store.get(offset).unwrap().topic
        );
    }

    // Cold rehydrate: fold every event from an empty aggregate.
    let position = replay(&store, Position::default(), apply_fill);
    println!(
        "  replay -> {} shares, cash {} cents",
        position.shares, position.cash_cents
    );
    assert_eq!(position.shares, 150, "100 + 50 bought");
    assert_eq!(position.cash_cents, -(100 * 18_800 + 50 * 19_010));

    // Live read model: catch_up folds only events appended since last time.
    let mut live = Projector::new(Position::default());
    live.catch_up(&store, apply_fill);
    assert_eq!(
        *live.state(),
        position,
        "catch_up agrees with a full replay"
    );
    let caught = live.position();
    println!("  projector caught up through offset {caught}");

    // A partial sell arrives; the projector pays only for the one new event.
    store.append(fill("sell", 40, 19_500, "10:15:00"));
    live.catch_up(&store, apply_fill);
    println!(
        "  after sell -> {} shares, cash {} cents",
        live.state().shares,
        live.state().cash_cents
    );
    assert_eq!(live.state().shares, 110, "150 - 40 sold");
    assert_eq!(
        live.position(),
        store.len() as u64,
        "consumed the whole log"
    );

    // Topic filter: audit just the fills without re-walking the aggregate.
    let fill_count = store.by_topic("order.filled").count();
    println!("  {fill_count} fills in the log");
    assert_eq!(fill_count, 3);

    // The subscriber saw every fill as it was appended (sync dispatch).
    assert_eq!(
        fills_seen.load(Ordering::SeqCst),
        3,
        "risk desk saw all fills"
    );
    println!(
        "  risk desk observed {} fills live",
        fills_seen.load(Ordering::SeqCst)
    );
}
