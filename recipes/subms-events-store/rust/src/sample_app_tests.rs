//! Pins the behaviour the `sample_app` example demonstrates: rebuilding a
//! trading position by folding the log, and the incremental projector agreeing
//! with a cold replay while only touching the tail.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
struct Position {
    shares: i64,
    cash_cents: i64,
}

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

fn fill(side: &str, qty: i64, price: i64) -> Event {
    Event::builder("order.filled")
        .at("t")
        .attr("side", side)
        .attr("qty", &qty.to_string())
        .attr("price", &price.to_string())
        .build()
}

fn seed(store: &mut EventStore) {
    store.append(
        Event::builder("order.submitted")
            .at("t")
            .attr("id", "A1")
            .build(),
    );
    store.append(fill("buy", 100, 18_800));
    store.append(fill("buy", 50, 19_010));
    store.append(
        Event::builder("order.canceled")
            .at("t")
            .attr("id", "A3")
            .build(),
    );
}

#[test]
fn replay_rebuilds_position() {
    let mut store = EventStore::new();
    seed(&mut store);
    let position = replay(&store, Position::default(), apply_fill);
    assert_eq!(position.shares, 150);
    assert_eq!(position.cash_cents, -(100 * 18_800 + 50 * 19_010));
}

#[test]
fn projector_agrees_with_replay_and_only_folds_tail() {
    let mut store = EventStore::new();
    seed(&mut store);

    let mut live = Projector::new(Position::default());
    live.catch_up(&store, apply_fill);
    let cold = replay(&store, Position::default(), apply_fill);
    assert_eq!(*live.state(), cold, "incremental matches a cold replay");
    assert_eq!(live.position(), store.len() as u64);

    store.append(fill("sell", 40, 19_500));
    live.catch_up(&store, apply_fill);
    assert_eq!(live.state().shares, 110);
    assert_eq!(
        live.position(),
        store.len() as u64,
        "consumed the whole log"
    );
}

#[test]
fn by_topic_counts_only_fills() {
    let mut store = EventStore::new();
    seed(&mut store);
    assert_eq!(store.by_topic("order.filled").count(), 2);
    assert_eq!(store.by_topic("order.submitted").count(), 1);
}

#[test]
fn subscriber_sees_every_fill_live() {
    let fills = Arc::new(AtomicU64::new(0));
    let counter = Arc::clone(&fills);
    let mut store = EventStore::new();
    store.subscribe(listener(move |e: &Event| {
        if e.topic == "order.filled" {
            counter.fetch_add(1, Ordering::SeqCst);
        }
    }));
    seed(&mut store);
    assert_eq!(
        fills.load(Ordering::SeqCst),
        2,
        "both fills observed on append"
    );
}
