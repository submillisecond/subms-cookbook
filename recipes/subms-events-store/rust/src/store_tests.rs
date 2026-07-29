use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use super::*;

fn ev(topic: &str) -> Event {
    Event::builder(topic).at("t").build()
}

#[test]
fn append_returns_offsets() {
    let mut s = EventStore::new();
    assert!(s.is_empty());
    assert_eq!(s.append(ev("a")), 0);
    assert_eq!(s.append(ev("b")), 1);
    assert_eq!(s.append(ev("c")), 2);
    assert_eq!(s.len(), 3);
    assert!(!s.is_empty());
}

#[test]
fn get_and_read_from() {
    let mut s = EventStore::new();
    s.append(ev("a"));
    s.append(ev("b"));
    s.append(ev("c"));
    assert_eq!(s.get(1).unwrap().topic, "b");
    assert!(s.get(9).is_none());
    let tail: Vec<&str> = s.read_from(1).iter().map(|e| e.topic.as_str()).collect();
    assert_eq!(tail, ["b", "c"]);
    assert!(s.read_from(9).is_empty());
}

#[test]
fn by_topic_filters() {
    let mut s = EventStore::new();
    s.append(ev("x"));
    s.append(ev("y"));
    s.append(ev("x"));
    assert_eq!(s.by_topic("x").count(), 2);
    assert_eq!(s.by_topic("z").count(), 0);
}

#[test]
fn empty_store_json() {
    assert_eq!(EventStore::new().to_json(), "[]");
}

#[test]
fn default_matches_new_and_events_accessor() {
    let mut s = EventStore::default();
    assert!(s.is_empty());
    s.append(ev("a"));
    s.append(ev("b"));
    let topics: Vec<&str> = s.events().iter().map(|e| e.topic.as_str()).collect();
    assert_eq!(topics, ["a", "b"]);
}

#[test]
fn replay_folds_whole_log() {
    let mut s = EventStore::new();
    s.append(ev("hit"));
    s.append(ev("miss"));
    s.append(ev("hit"));
    let hits = replay(&s, 0u64, |n, e| {
        if e.topic == "hit" {
            *n += 1;
        }
    });
    assert_eq!(hits, 2);
}

#[test]
fn replay_over_empty_is_initial() {
    let s = EventStore::new();
    assert_eq!(replay(&s, 42u64, |n, _e| *n += 1), 42);
}

#[test]
fn projector_catches_up_incrementally() {
    let mut s = EventStore::new();
    s.append(ev("a"));
    s.append(ev("b"));
    let mut p = Projector::new(0u64);
    p.catch_up(&s, |n, _e| *n += 1);
    assert_eq!(*p.state(), 2);
    assert_eq!(p.position(), 2);
    // appending then catching up applies only the tail
    s.append(ev("c"));
    p.catch_up(&s, |n, _e| *n += 1);
    assert_eq!(*p.state(), 3);
    assert_eq!(p.position(), 3);
}

#[test]
fn projector_catch_up_twice_is_noop_without_new() {
    let mut s = EventStore::new();
    s.append(ev("a"));
    let mut p = Projector::new(0u64);
    p.catch_up(&s, |n, _e| *n += 1);
    p.catch_up(&s, |n, _e| *n += 1);
    assert_eq!(*p.state(), 1);
}

#[test]
fn projector_into_state() {
    let mut s = EventStore::new();
    s.append(ev("a"));
    let mut p = Projector::new(String::new());
    p.catch_up(&s, |acc, e| acc.push_str(&e.topic));
    assert_eq!(p.into_state(), "a");
}

#[test]
fn subscribe_sync_delivers_on_append() {
    let hits = Arc::new(AtomicU64::new(0));
    let h = Arc::clone(&hits);
    let mut s = EventStore::new();
    s.subscribe(listener(move |_e| {
        h.fetch_add(1, Ordering::SeqCst);
    }));
    s.append(ev("a"));
    s.append(ev("b"));
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn subscribe_async_delivers() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let mut s = EventStore::with_dispatch(DispatchMode::Async);
    s.subscribe(listener(move |e: &Event| {
        sink.lock().unwrap().push(e.topic.clone())
    }));
    s.append(ev("a"));
    s.append(ev("b"));
    for _ in 0..100 {
        if seen.lock().unwrap().len() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(seen.lock().unwrap().len(), 2);
}

#[test]
fn cross_language_store_fixture() {
    let mut s = EventStore::new();
    s.append(
        Event::builder("user.created")
            .at("2026-06-18T00:00:00Z")
            .attr("id", "7")
            .build(),
    );
    s.append(
        Event::builder("user.renamed")
            .at("2026-06-18T00:00:01Z")
            .attr("id", "7")
            .attr("name", "ko")
            .build(),
    );
    assert_eq!(
        s.to_json(),
        "[{\"topic\":\"user.created\",\"level\":\"INFO\",\"at\":\"2026-06-18T00:00:00Z\",\"attributes\":{\"id\":\"7\"}},{\"topic\":\"user.renamed\",\"level\":\"INFO\",\"at\":\"2026-06-18T00:00:01Z\",\"attributes\":{\"id\":\"7\",\"name\":\"ko\"}}]"
    );
}

#[test]
fn stress_append_then_replay_count() {
    let mut s = EventStore::new();
    for _ in 0..10_000 {
        s.append(ev("e"));
    }
    let n = replay(&s, 0u64, |n, _e| *n += 1);
    assert_eq!(n, 10_000);
    assert_eq!(s.len(), 10_000);
}
