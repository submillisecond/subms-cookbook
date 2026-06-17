use subms_ts::TsSeriesMetadata;
use subms_ts_cdc::{TsChangeEvent, TsObservableCollection};

fn obs() -> TsObservableCollection<f64> {
    TsObservableCollection::new()
}

fn register(o: &mut TsObservableCollection<f64>, id: u64, name: &str) -> u64 {
    o.register(TsSeriesMetadata::new(id, name)).unwrap()
}

#[test]
fn subscribe_then_push_yields_push_event() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    let id = register(&mut o, 1, "a");
    o.push(id, 1_000, 42.5).unwrap();
    assert_eq!(
        sub.try_recv(),
        Some(TsChangeEvent::Push {
            series_id: id,
            ts: 1_000,
            value: 42.5
        })
    );
    assert_eq!(sub.try_recv(), None);
}

#[test]
fn delete_at_fires_delete_at_event() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    let id = register(&mut o, 1, "a");
    o.push(id, 10, 1.0).unwrap();
    assert!(matches!(
        sub.try_recv(),
        Some(TsChangeEvent::Push { ts: 10, .. })
    ));
    let removed = o.delete_at(id, 10);
    assert!(removed.is_some());
    assert_eq!(sub.try_recv(), Some(TsChangeEvent::DeleteAt { series_id: id, ts: 10 }));
}

#[test]
fn delete_at_missing_point_publishes_nothing() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    let id = register(&mut o, 1, "a");
    assert!(o.delete_at(id, 999).is_none());
    assert_eq!(sub.try_recv(), None);
}

#[test]
fn delete_range_fires_delete_range_event() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    let id = register(&mut o, 1, "a");
    o.push(id, 1, 1.0).unwrap();
    o.push(id, 2, 2.0).unwrap();
    o.push(id, 3, 3.0).unwrap();
    let _ = sub.drain();
    let n = o.delete_range(id, 1, 2);
    assert_eq!(n, 2);
    assert_eq!(
        sub.try_recv(),
        Some(TsChangeEvent::DeleteRange {
            series_id: id,
            lo: 1,
            hi: 2
        })
    );
}

#[test]
fn delete_range_empty_publishes_nothing() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    let id = register(&mut o, 1, "a");
    let n = o.delete_range(id, 100, 200);
    assert_eq!(n, 0);
    assert_eq!(sub.try_recv(), None);
}

#[test]
fn deregister_fires_deregister_event() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    let id = register(&mut o, 7, "a");
    o.push(id, 1, 1.0).unwrap();
    let _ = sub.drain();
    let s = o.deregister(id);
    assert!(s.is_some());
    assert_eq!(sub.try_recv(), Some(TsChangeEvent::Deregister { series_id: 7 }));
}

#[test]
fn deregister_unknown_publishes_nothing() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    assert!(o.deregister(404).is_none());
    assert_eq!(sub.try_recv(), None);
}

#[test]
fn two_subscribers_both_receive_same_event() {
    let mut o = obs();
    let mut a = o.subscribe(16);
    let mut b = o.subscribe(16);
    let id = register(&mut o, 1, "s");
    o.push(id, 5, 9.0).unwrap();
    let want = TsChangeEvent::Push {
        series_id: id,
        ts: 5,
        value: 9.0,
    };
    assert_eq!(a.try_recv(), Some(want));
    assert_eq!(b.try_recv(), Some(want));
}

#[test]
fn no_subscriber_path_is_silent_and_lossless() {
    let mut o = obs();
    let id = register(&mut o, 1, "s");
    for t in 0..1_000i64 {
        o.push(id, t, t as f64).unwrap();
    }
    assert_eq!(o.dropped_events(), 0);
    assert_eq!(o.subscriber_count(), 0);
    assert_eq!(o.collection().get(id).unwrap().len(), 1_000);
}

#[test]
fn ring_full_drops_events_but_collection_keeps_all_data() {
    let mut o = obs();
    // Requested 4 -> power-of-two ring of 4 slots. Never drained.
    let _sub = o.subscribe(4);
    let id = register(&mut o, 1, "s");
    let total = 100i64;
    for t in 0..total {
        o.push(id, t, t as f64).unwrap();
    }
    assert!(o.dropped_events() > 0, "a full ring must drop");
    // The underlying collection is untouched by the drop policy.
    assert_eq!(o.collection().get(id).unwrap().len(), total as usize);
}

#[test]
fn dropped_increments_per_full_ring_per_event() {
    let mut o = obs();
    let _a = o.subscribe(2);
    let _b = o.subscribe(2);
    let id = register(&mut o, 1, "s");
    // Two rings of 2 slots each: 4 events land, the rest drop on both rings.
    for t in 0..10i64 {
        o.push(id, t, t as f64).unwrap();
    }
    // 10 events * 2 rings = 20 publishes, 4 fit -> 16 drops.
    assert_eq!(o.dropped_events(), 16);
}

#[test]
fn drain_returns_fifo_order() {
    let mut o = obs();
    let mut sub = o.subscribe(64);
    let id = register(&mut o, 1, "s");
    for t in 0..5i64 {
        o.push(id, t, (t * 10) as f64).unwrap();
    }
    let events = sub.drain();
    assert_eq!(events.len(), 5);
    for (t, ev) in events.iter().enumerate() {
        assert_eq!(
            *ev,
            TsChangeEvent::Push {
                series_id: id,
                ts: t as i64,
                value: (t as i64 * 10) as f64
            }
        );
    }
}

#[test]
fn drain_on_empty_ring_is_empty() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    assert!(sub.drain().is_empty());
}

#[test]
fn read_through_reflects_mutations() {
    let mut o = obs();
    let id = register(&mut o, 1, "px");
    o.push(id, 1, 1.0).unwrap();
    o.push(id, 2, 2.0).unwrap();
    assert_eq!(o.collection().by_name("px").unwrap().len(), 2);
    o.delete_at(id, 1);
    assert_eq!(o.collection().get(id).unwrap().len(), 1);
}

#[test]
fn all_event_types_in_one_session() {
    let mut o = obs();
    let mut sub = o.subscribe(64);
    let id = register(&mut o, 3, "s");
    o.push(id, 1, 1.0).unwrap();
    o.push(id, 2, 2.0).unwrap();
    o.push(id, 3, 3.0).unwrap();
    o.delete_at(id, 1);
    o.delete_range(id, 2, 3);
    o.deregister(id);
    let evs = sub.drain();
    assert_eq!(evs.len(), 6);
    assert!(matches!(evs[0], TsChangeEvent::Push { ts: 1, .. }));
    assert!(matches!(evs[3], TsChangeEvent::DeleteAt { ts: 1, .. }));
    assert!(matches!(evs[4], TsChangeEvent::DeleteRange { lo: 2, hi: 3, .. }));
    assert!(matches!(evs[5], TsChangeEvent::Deregister { series_id: 3 }));
}

#[test]
fn default_and_new_agree() {
    let a: TsObservableCollection<f64> = TsObservableCollection::default();
    let b: TsObservableCollection<f64> = TsObservableCollection::new();
    assert_eq!(a.subscriber_count(), b.subscriber_count());
    assert_eq!(a.dropped_events(), b.dropped_events());
    assert!(a.collection().is_empty());
}

#[test]
fn register_publishes_nothing() {
    let mut o = obs();
    let mut sub = o.subscribe(16);
    register(&mut o, 1, "s");
    assert_eq!(sub.try_recv(), None);
}
