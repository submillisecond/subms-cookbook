//! Zero-dep demo: append a few events, fold a read model, show the JSON log.
//! Run: `cargo run --example demo`.

use subms_events_store::{Event, EventStore, replay};

fn main() {
    let mut store = EventStore::new();
    store.append(
        Event::builder("user.created")
            .at("t0")
            .attr("id", "7")
            .build(),
    );
    store.append(
        Event::builder("user.renamed")
            .at("t1")
            .attr("id", "7")
            .attr("name", "ko")
            .build(),
    );
    store.append(
        Event::builder("user.created")
            .at("t2")
            .attr("id", "8")
            .build(),
    );

    let created = replay(&store, 0u64, |n, e| {
        if e.topic == "user.created" {
            *n += 1;
        }
    });
    println!("users created: {created}");
    println!("log: {}", store.to_json());
}
