//! Property test: an incremental `Projector` must agree with a full `replay` at
//! every catch-up point, for any random interleaving of appends and catch-ups.
//! This is the core event-sourcing invariant (the read model never drifts).

use subms_events_store::{Event, EventStore, Projector, replay};

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

#[test]
fn prop_incremental_projection_equals_full_replay() {
    let mut rng = Rng::new(7);
    for _ in 0..300 {
        let mut store = EventStore::new();
        let mut proj = Projector::new(0u64);
        let ops = rng.below(40);
        for _ in 0..ops {
            if rng.below(3) != 0 {
                store.append(Event::builder(&format!("t{}", rng.below(4))).at("t").build());
            } else {
                proj.catch_up(&store, |n, _e| *n += 1);
                let full = replay(&store, 0u64, |n, _e| *n += 1);
                assert_eq!(*proj.state(), full);
                assert_eq!(proj.position(), store.len() as u64);
            }
        }
        // Final catch-up always reaches the full count.
        proj.catch_up(&store, |n, _e| *n += 1);
        assert_eq!(*proj.state(), store.len() as u64);
        assert_eq!(proj.position(), store.len() as u64);
    }
}

#[test]
fn prop_offsets_are_dense_and_monotonic() {
    let mut rng = Rng::new(13);
    for _ in 0..200 {
        let mut store = EventStore::new();
        let n = rng.below(50);
        for i in 0..n {
            let off = store.append(Event::builder("e").at("t").build());
            assert_eq!(off, i); // append returns a dense, monotonic offset
        }
        assert_eq!(store.len() as u64, n);
        // by_topic is always a subsequence of the full log.
        let total: usize = ["e", "x"].iter().map(|t| store.by_topic(t).count()).sum();
        assert!(total <= store.len());
    }
}
