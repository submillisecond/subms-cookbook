use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use super::*;

#[test]
fn event_builder_and_accessors() {
    let e = Event::builder("cache.evict")
        .level(EventLevel::Warn)
        .at("2026-06-18T00:00:00Z")
        .message("evicted")
        .attr("keys", "128")
        .build();
    assert_eq!(e.topic, "cache.evict");
    assert_eq!(e.level, EventLevel::Warn);
    assert_eq!(e.attr("keys"), Some("128"));
    assert_eq!(e.attr("missing"), None);
}

#[test]
fn event_level_tokens() {
    assert_eq!(EventLevel::Trace.as_str(), "TRACE");
    assert_eq!(EventLevel::Debug.as_str(), "DEBUG");
    assert_eq!(EventLevel::Info.as_str(), "INFO");
    assert_eq!(EventLevel::Warn.as_str(), "WARN");
    assert_eq!(EventLevel::Error.as_str(), "ERROR");
}

#[test]
fn transition_helper() {
    let e = Event::transition("svc.status", EventLevel::Error, "db", "UP", "DOWN");
    assert_eq!(e.attr("scope"), Some("db"));
    assert_eq!(e.attr("from"), Some("UP"));
    assert_eq!(e.attr("to"), Some("DOWN"));
}

#[test]
fn cross_language_event_fixture() {
    let e = Event::builder("svc.status")
        .level(EventLevel::Error)
        .at("2026-06-18T00:00:00Z")
        .message("db down")
        .attr("from", "UP")
        .attr("to", "DOWN")
        .build();
    assert_eq!(
        e.to_json(),
        "{\"topic\":\"svc.status\",\"level\":\"ERROR\",\"at\":\"2026-06-18T00:00:00Z\",\"message\":\"db down\",\"attributes\":{\"from\":\"UP\",\"to\":\"DOWN\"}}"
    );
}

#[test]
fn json_omits_absent_fields() {
    let e = Event::builder("x").at("T").build();
    assert_eq!(
        e.to_json(),
        "{\"topic\":\"x\",\"level\":\"INFO\",\"at\":\"T\"}"
    );
}

#[test]
fn json_escaping() {
    let e = Event::builder("x").at("T").message("a\"b\\c\nd\te").build();
    assert!(e.to_json().contains("a\\\"b\\\\c\\nd\\te"));
}

#[test]
fn sync_dispatch_inline() {
    let hits = Arc::new(AtomicU64::new(0));
    let h = Arc::clone(&hits);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(listener(move |_e| {
        h.fetch_add(1, Ordering::SeqCst);
    }));
    bus.emit(Event::builder("a").build());
    bus.emit(Event::builder("b").build());
    // Sync dispatch is inline: the count is already updated, no waiting.
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

#[test]
fn async_dispatch_off_thread() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(move |e: &Event| {
        sink.lock().unwrap().push(e.topic.clone())
    }));
    bus.emit(Event::builder("a").build());
    bus.emit(Event::builder("b").build());
    for _ in 0..100 {
        if seen.lock().unwrap().len() >= 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    bus.stop();
    let got = seen.lock().unwrap().clone();
    assert_eq!(got.len(), 2);
    assert!(got.contains(&"a".to_string()) && got.contains(&"b".to_string()));
}

#[test]
fn composite_fan_out() {
    let a = Arc::new(AtomicU64::new(0));
    let b = Arc::new(AtomicU64::new(0));
    let (a2, b2) = (Arc::clone(&a), Arc::clone(&b));
    let composite = CompositeListener::new(vec![
        listener(move |_e| {
            a2.fetch_add(1, Ordering::SeqCst);
        }),
        listener(move |_e| {
            b2.fetch_add(1, Ordering::SeqCst);
        }),
    ]);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(Arc::new(composite));
    bus.emit(Event::builder("x").build());
    assert_eq!(a.load(Ordering::SeqCst), 1);
    assert_eq!(b.load(Ordering::SeqCst), 1);
}

#[test]
fn filter_gate() {
    let hits = Arc::new(AtomicU64::new(0));
    let h = Arc::clone(&hits);
    let inner = listener(move |_e| {
        h.fetch_add(1, Ordering::SeqCst);
    });
    let filtered = FilterListener::new(|e: &Event| e.level == EventLevel::Error, inner);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(Arc::new(filtered));
    bus.emit(Event::builder("a").level(EventLevel::Info).build()); // dropped
    bus.emit(Event::builder("b").level(EventLevel::Error).build()); // passes
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn bridge_receives_events() {
    use crate::{EventBridge, EventDispatcher};

    struct CountingBridge {
        n: AtomicU64,
    }
    impl EventBridge for CountingBridge {
        fn name(&self) -> &str {
            "counting"
        }
        fn forward(&self, _event: &Event) {
            self.n.fetch_add(1, Ordering::SeqCst);
        }
    }
    let bridge = Arc::new(CountingBridge {
        n: AtomicU64::new(0),
    });
    let mut bus = EventDispatcher::sync();
    bus.add_bridge(Arc::clone(&bridge) as Arc<dyn EventBridge>);
    bus.emit(Event::builder("x").build());
    bus.emit(Event::builder("y").build());
    assert_eq!(bridge.n.load(Ordering::SeqCst), 2);
}

#[test]
fn no_listener_emit_is_noop() {
    let mut bus = EventDispatcher::asynchronous();
    // No listener registered, dispatcher thread never started; emit must not panic.
    bus.emit(Event::builder("x").build());
    assert_eq!(bus.listener_count(), 0);
    bus.stop();
}

#[test]
fn listener_count_tracks_registrations() {
    let mut bus = EventDispatcher::sync();
    assert_eq!(bus.listener_count(), 0);
    bus.add_listener(listener(|_e| {}));
    bus.add_listener(listener(|_e| {}));
    assert_eq!(bus.listener_count(), 2);
}

#[test]
fn emit_handle_emits_from_a_producer() {
    let hits = Arc::new(AtomicU64::new(0));
    let h = Arc::clone(&hits);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(listener(move |_e| {
        h.fetch_add(1, Ordering::SeqCst);
    }));
    let handle = bus.handle(); // cloneable emitter, doesn't own the dispatcher
    handle.emit(Event::builder("a").build());
    handle.clone().emit(Event::builder("b").build());
    assert_eq!(hits.load(Ordering::SeqCst), 2);
}

// A gated listener: the first event blocks on a held Mutex so the consumer
// thread parks and the bounded queue fills deterministically. Returns the
// delivered-topics sink + the "entered" counter.
type GatedListener = (
    Arc<dyn crate::EventListener>,
    Arc<Mutex<Vec<String>>>,
    Arc<AtomicU64>,
);

fn gated_listener(gate: Arc<Mutex<()>>) -> GatedListener {
    let entered = Arc::new(AtomicU64::new(0));
    let delivered: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::new(AtomicBool::new(true));
    let (g2, e2, d2, f2) = (
        Arc::clone(&gate),
        Arc::clone(&entered),
        Arc::clone(&delivered),
        Arc::clone(&first),
    );
    let l = listener(move |ev: &Event| {
        if f2.swap(false, Ordering::SeqCst) {
            e2.fetch_add(1, Ordering::SeqCst);
            let _hold = g2.lock().unwrap(); // parks until the test releases the gate
            d2.lock().unwrap().push(ev.topic.clone());
        } else {
            d2.lock().unwrap().push(ev.topic.clone());
        }
    });
    (l, delivered, entered)
}

fn wait_for<F: Fn() -> bool>(cond: F) {
    for _ in 0..400 {
        if cond() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

#[test]
fn bounded_drop_newest_drops_when_full() {
    let gate = Arc::new(Mutex::new(()));
    let held = gate.lock().unwrap();
    let (l, delivered, entered) = gated_listener(Arc::clone(&gate));
    let mut bus = EventDispatcher::bounded(2, OverflowPolicy::DropNewest);
    bus.add_listener(l);

    bus.emit(Event::builder("e1").build());
    wait_for(|| entered.load(Ordering::SeqCst) >= 1); // consumer parked on e1
    bus.emit(Event::builder("e2").build()); // queue [e2]
    bus.emit(Event::builder("e3").build()); // queue [e2, e3] (full)
    bus.emit(Event::builder("e4").build()); // full -> dropped
    assert_eq!(bus.dropped(), 1);

    drop(held);
    wait_for(|| delivered.lock().unwrap().len() >= 3);
    bus.stop();
    assert_eq!(delivered.lock().unwrap().as_slice(), ["e1", "e2", "e3"]);
}

#[test]
fn bounded_drop_oldest_evicts_stalest() {
    let gate = Arc::new(Mutex::new(()));
    let held = gate.lock().unwrap();
    let (l, delivered, entered) = gated_listener(Arc::clone(&gate));
    let mut bus = EventDispatcher::bounded(2, OverflowPolicy::DropOldest);
    bus.add_listener(l);

    bus.emit(Event::builder("e1").build());
    wait_for(|| entered.load(Ordering::SeqCst) >= 1);
    bus.emit(Event::builder("e2").build()); // [e2]
    bus.emit(Event::builder("e3").build()); // [e2, e3]
    bus.emit(Event::builder("e4").build()); // evicts e2 -> [e3, e4]
    assert_eq!(bus.dropped(), 1);

    drop(held);
    wait_for(|| delivered.lock().unwrap().len() >= 3);
    bus.stop();
    assert_eq!(delivered.lock().unwrap().as_slice(), ["e1", "e3", "e4"]); // e2 evicted
}

#[test]
fn bounded_block_delivers_without_dropping() {
    let n = Arc::new(AtomicU64::new(0));
    let c = Arc::clone(&n);
    let mut bus = EventDispatcher::bounded(8, OverflowPolicy::Block);
    bus.add_listener(listener(move |_e| {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    for _ in 0..4 {
        bus.emit(Event::builder("x").build());
    }
    wait_for(|| n.load(Ordering::SeqCst) >= 4);
    bus.stop();
    assert_eq!(n.load(Ordering::SeqCst), 4);
    assert_eq!(bus.dropped(), 0);
}

#[test]
fn unbounded_async_never_drops() {
    let n = Arc::new(AtomicU64::new(0));
    let c = Arc::clone(&n);
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(move |_e| {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    for _ in 0..1_000 {
        bus.emit(Event::builder("x").build());
    }
    wait_for(|| n.load(Ordering::SeqCst) >= 1_000);
    bus.stop();
    assert_eq!(bus.dropped(), 0);
}

// House concurrency standard (CLAUDE.md): >= 4-8 threads, 1M+ ops, assert an
// invariant. These sample real interleavings on the shared EmitHandle.

#[test]
fn stress_multi_producer_unbounded_no_loss() {
    let n = Arc::new(AtomicU64::new(0));
    let c = Arc::clone(&n);
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(move |_e| {
        c.fetch_add(1, Ordering::Relaxed);
    }));
    let handle = bus.handle();
    let producers = 8u64;
    let per = 200_000u64;
    let total = producers * per; // 1.6M ops
    let mut hs = Vec::new();
    for _ in 0..producers {
        let h = handle.clone();
        hs.push(std::thread::spawn(move || {
            for _ in 0..per {
                h.emit(Event::builder("x").build());
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    wait_for(|| n.load(Ordering::Relaxed) >= total);
    bus.stop();
    // Unbounded async: every emitted event is delivered exactly once.
    assert_eq!(n.load(Ordering::Relaxed), total);
}

#[test]
fn stress_bounded_drop_conserves_every_event() {
    let n = Arc::new(AtomicU64::new(0));
    let c = Arc::clone(&n);
    let mut bus = EventDispatcher::bounded(1024, OverflowPolicy::DropNewest);
    bus.add_listener(listener(move |_e| {
        c.fetch_add(1, Ordering::Relaxed);
    }));
    let handle = bus.handle();
    let producers = 8u64;
    let per = 150_000u64;
    let total = producers * per; // 1.2M ops, queue cap 1024 -> heavy dropping
    let mut hs = Vec::new();
    for _ in 0..producers {
        let h = handle.clone();
        hs.push(std::thread::spawn(move || {
            for _ in 0..per {
                h.emit(Event::builder("x").build());
            }
        }));
    }
    for h in hs {
        h.join().unwrap();
    }
    wait_for(|| n.load(Ordering::Relaxed) + bus.dropped() >= total);
    bus.stop();
    // Conservation: under contention every event is either delivered or dropped,
    // never silently lost or double-counted.
    assert_eq!(n.load(Ordering::Relaxed) + bus.dropped(), total);
}

#[test]
fn dispatcher_mode_accessor() {
    use crate::DispatchMode;
    assert_eq!(EventDispatcher::sync().mode(), DispatchMode::Sync);
    assert_eq!(EventDispatcher::asynchronous().mode(), DispatchMode::Async);
}

#[test]
fn async_preserves_emission_order() {
    let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(move |e: &Event| {
        sink.lock().unwrap().push(e.topic.clone())
    }));
    for t in ["a", "b", "c", "d"] {
        bus.emit(Event::builder(t).build());
    }
    for _ in 0..200 {
        if seen.lock().unwrap().len() >= 4 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    bus.stop();
    assert_eq!(seen.lock().unwrap().as_slice(), ["a", "b", "c", "d"]); // mpsc is FIFO
}

#[test]
fn async_stress_delivers_every_event() {
    let n = Arc::new(AtomicU64::new(0));
    let c = Arc::clone(&n);
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(move |_e| {
        c.fetch_add(1, Ordering::SeqCst);
    }));
    for _ in 0..5_000 {
        bus.emit(Event::builder("x").build());
    }
    for _ in 0..400 {
        if n.load(Ordering::SeqCst) >= 5_000 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    bus.stop();
    assert_eq!(n.load(Ordering::SeqCst), 5_000);
}

#[test]
fn multiple_async_listeners_all_receive() {
    let a = Arc::new(AtomicU64::new(0));
    let b = Arc::new(AtomicU64::new(0));
    let (a2, b2) = (Arc::clone(&a), Arc::clone(&b));
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(move |_e| {
        a2.fetch_add(1, Ordering::SeqCst);
    }));
    bus.add_listener(listener(move |_e| {
        b2.fetch_add(1, Ordering::SeqCst);
    }));
    bus.emit(Event::builder("x").build());
    for _ in 0..200 {
        if a.load(Ordering::SeqCst) >= 1 && b.load(Ordering::SeqCst) >= 1 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    bus.stop();
    assert_eq!(a.load(Ordering::SeqCst), 1);
    assert_eq!(b.load(Ordering::SeqCst), 1);
}

#[test]
fn stop_is_idempotent_and_emit_after_stop_is_safe() {
    let mut bus = EventDispatcher::asynchronous();
    bus.add_listener(listener(|_e| {}));
    bus.emit(Event::builder("x").build());
    bus.stop();
    bus.stop(); // second stop must not panic
    bus.emit(Event::builder("y").build()); // emit after stop: dropped, no panic
}

#[test]
fn sync_emit_before_listener_is_not_seen() {
    let hits = Arc::new(AtomicU64::new(0));
    let h = Arc::clone(&hits);
    let mut bus = EventDispatcher::sync();
    bus.emit(Event::builder("early").build()); // no listener yet -> nothing
    bus.add_listener(listener(move |_e| {
        h.fetch_add(1, Ordering::SeqCst);
    }));
    bus.emit(Event::builder("late").build());
    assert_eq!(hits.load(Ordering::SeqCst), 1);
}

#[test]
fn nested_filter_over_composite() {
    let a = Arc::new(AtomicU64::new(0));
    let b = Arc::new(AtomicU64::new(0));
    let (a2, b2) = (Arc::clone(&a), Arc::clone(&b));
    let composite = Arc::new(CompositeListener::new(vec![
        listener(move |_e| {
            a2.fetch_add(1, Ordering::SeqCst);
        }),
        listener(move |_e| {
            b2.fetch_add(1, Ordering::SeqCst);
        }),
    ]));
    let gated = FilterListener::new(|e: &Event| e.level == EventLevel::Error, composite);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(Arc::new(gated));
    bus.emit(Event::builder("ok").level(EventLevel::Info).build()); // gated out
    bus.emit(Event::builder("bad").level(EventLevel::Error).build()); // passes to both
    assert_eq!(a.load(Ordering::SeqCst), 1);
    assert_eq!(b.load(Ordering::SeqCst), 1);
}

#[test]
fn json_sorts_multiple_attributes() {
    let e = Event::builder("t")
        .at("T")
        .attr("zeta", "1")
        .attr("alpha", "2")
        .attr("mid", "3")
        .build();
    assert_eq!(
        e.to_json(),
        "{\"topic\":\"t\",\"level\":\"INFO\",\"at\":\"T\",\"attributes\":{\"alpha\":\"2\",\"mid\":\"3\",\"zeta\":\"1\"}}"
    );
}

#[test]
fn event_clone_equality() {
    let e = Event::transition("svc", EventLevel::Warn, "x", "UP", "WARN");
    assert_eq!(e, e.clone());
}

#[test]
fn builder_last_write_wins() {
    let e = Event::builder("t")
        .level(EventLevel::Info)
        .level(EventLevel::Error)
        .message("first")
        .message("second")
        .attr("k", "1")
        .attr("k", "2")
        .build();
    assert_eq!(e.level, EventLevel::Error);
    assert_eq!(e.message.as_deref(), Some("second"));
    assert_eq!(e.attr("k"), Some("2"));
}

#[test]
fn bridge_listener_exposes_name_and_flush_is_noop() {
    use crate::{BridgeListener, EventBridge};

    struct NamedBridge;
    impl EventBridge for NamedBridge {
        fn name(&self) -> &str {
            "named"
        }
        fn forward(&self, _event: &Event) {}
    }
    let bridge: Arc<dyn EventBridge> = Arc::new(NamedBridge);
    bridge.flush(); // default no-op, must not panic
    let bl = BridgeListener::new(bridge);
    assert_eq!(bl.name(), "named");
}

#[test]
fn composite_push_appends_listener() {
    let a = Arc::new(AtomicU64::new(0));
    let b = Arc::new(AtomicU64::new(0));
    let (a2, b2) = (Arc::clone(&a), Arc::clone(&b));
    let mut composite = CompositeListener::new(vec![listener(move |_e| {
        a2.fetch_add(1, Ordering::SeqCst);
    })]);
    composite.push(listener(move |_e| {
        b2.fetch_add(1, Ordering::SeqCst);
    }));
    let mut bus = EventDispatcher::sync();
    bus.add_listener(Arc::new(composite));
    bus.emit(Event::builder("x").build());
    assert_eq!(a.load(Ordering::SeqCst), 1);
    assert_eq!(b.load(Ordering::SeqCst), 1);
}

#[test]
fn emit_handle_reports_mode() {
    use crate::DispatchMode;
    assert_eq!(EventDispatcher::sync().handle().mode(), DispatchMode::Sync);
    assert_eq!(
        EventDispatcher::asynchronous().handle().mode(),
        DispatchMode::Async
    );
}

#[test]
fn filter_dropping_all_delivers_nothing() {
    let hits = Arc::new(AtomicU64::new(0));
    let h = Arc::clone(&hits);
    let inner = listener(move |_e| {
        h.fetch_add(1, Ordering::SeqCst);
    });
    let never = FilterListener::new(|_e: &Event| false, inner);
    let mut bus = EventDispatcher::sync();
    bus.add_listener(Arc::new(never));
    for _ in 0..10 {
        bus.emit(Event::builder("x").level(EventLevel::Error).build());
    }
    assert_eq!(hits.load(Ordering::SeqCst), 0);
}
