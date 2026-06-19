//! Property-based invariant tests over randomized scenarios (seeded, zero-dep
//! xorshift). Sync dispatch keeps them deterministic - the invariant must hold
//! for every generated input, not just the hand-picked cases.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use subms_events::{
    CompositeListener, Event, EventDispatcher, EventLevel, FilterListener, listener,
};

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

const LEVELS: [EventLevel; 3] = [EventLevel::Info, EventLevel::Warn, EventLevel::Error];

#[test]
fn prop_sync_delivers_full_sequence_in_order() {
    let mut rng = Rng::new(1);
    for _ in 0..500 {
        let len = rng.below(20) as usize;
        let topics: Vec<String> = (0..len).map(|_| format!("t{}", rng.below(5))).collect();
        let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let s = Arc::clone(&seen);
        let mut bus = EventDispatcher::sync();
        bus.add_listener(listener(move |e: &Event| {
            s.lock().unwrap().push(e.topic.clone())
        }));
        for t in &topics {
            bus.emit(Event::builder(t).build());
        }
        assert_eq!(*seen.lock().unwrap(), topics);
    }
}

#[test]
fn prop_filter_forwards_exactly_matching() {
    let mut rng = Rng::new(2);
    for _ in 0..500 {
        let target = LEVELS[rng.below(3) as usize];
        let len = rng.below(30) as usize;
        let evs: Vec<EventLevel> = (0..len).map(|_| LEVELS[rng.below(3) as usize]).collect();
        let cnt = Arc::new(AtomicU64::new(0));
        let c = Arc::clone(&cnt);
        let inner = listener(move |_e| {
            c.fetch_add(1, Ordering::Relaxed);
        });
        let f = FilterListener::new(move |e: &Event| e.level == target, inner);
        let mut bus = EventDispatcher::sync();
        bus.add_listener(Arc::new(f));
        for lv in &evs {
            bus.emit(Event::builder("x").level(*lv).build());
        }
        let expected = evs.iter().filter(|&&l| l == target).count() as u64;
        assert_eq!(cnt.load(Ordering::Relaxed), expected);
    }
}

#[test]
fn prop_composite_each_child_sees_all() {
    let mut rng = Rng::new(3);
    for _ in 0..400 {
        let children = (1 + rng.below(5)) as usize;
        let emits = rng.below(25);
        let counters: Vec<Arc<AtomicU64>> =
            (0..children).map(|_| Arc::new(AtomicU64::new(0))).collect();
        let listeners: Vec<_> = counters
            .iter()
            .map(|c| {
                let c = Arc::clone(c);
                listener(move |_e| {
                    c.fetch_add(1, Ordering::Relaxed);
                })
            })
            .collect();
        let composite = CompositeListener::new(listeners);
        let mut bus = EventDispatcher::sync();
        bus.add_listener(Arc::new(composite));
        for _ in 0..emits {
            bus.emit(Event::builder("x").build());
        }
        for c in &counters {
            assert_eq!(c.load(Ordering::Relaxed), emits);
        }
    }
}
