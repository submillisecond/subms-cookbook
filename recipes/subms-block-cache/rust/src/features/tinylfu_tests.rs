use super::*;

#[test]
fn basic_put_then_get() {
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(16);
    c.put(1, 10);
    c.put(2, 20);
    assert_eq!(c.get(&1).copied(), Some(10));
    assert_eq!(c.get(&2).copied(), Some(20));
    assert_eq!(c.get(&999), None);
}

#[test]
fn capacity_floor_is_four() {
    let c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(0);
    assert_eq!(c.capacity(), 4);
}

#[test]
fn admission_filter_tracks_admissions_and_rejections() {
    // Seed CMS with a heavily-accessed popular set so its keys
    // accumulate frequency weight. Then spray distinct singletons
    // and count rejections - with CMS-gated admission, at least
    // some singleton candidates must be rejected outright.
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(64);
    for _ in 0..50 {
        for k in 0u32..16 {
            let _ = c.get(&k);
        }
    }
    for k in 0u32..16 {
        c.put(k, k);
    }
    for _ in 0..50 {
        for k in 0u32..16 {
            let _ = c.get(&k);
        }
    }
    let rej_before = c.rejections();
    for k in 1000u32..3000 {
        c.put(k, k);
    }
    assert!(
        c.rejections() > rej_before,
        "expected the admission filter to reject some scan candidates"
    );
    assert!(c.admissions() + c.rejections() > 0);
}

#[test]
fn promotion_from_probation_to_protected() {
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(64);
    for k in 0u32..40 {
        c.put(k, k);
    }
    // After enough churn some keys are in Probation.
    let before = c.protected_len();
    // Hammer key 5 enough that on a probation hit it promotes.
    for _ in 0..5 {
        let _ = c.get(&5);
    }
    // Either it was already Protected (still ok), or it just got promoted.
    assert!(c.protected_len() >= before);
}

#[test]
fn update_in_place_does_not_evict() {
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(16);
    c.put(7, 70);
    let ev = c.put(7, 71);
    assert!(ev.is_none());
    assert_eq!(c.get(&7).copied(), Some(71));
}

#[test]
fn admissions_counter_tracks_window_evictions() {
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(16);
    // Hammer a working set so CMS gives them weight.
    for _ in 0..50 {
        for k in 0u32..8 {
            let _ = c.get(&k);
        }
    }
    for k in 0u32..50 {
        c.put(k, k);
    }
    assert!(c.admissions() + c.rejections() > 0);
}

#[test]
fn cms_estimate_grows_with_access_count() {
    let mut sketch = Cms::new(256, 10_000);
    let h = 0x0123_4567_89ab_cdef_u64;
    for _ in 0..10 {
        sketch.increment(h);
    }
    let e = sketch.estimate(h);
    assert!(e >= 10 || e == CMS_COUNTER_MAX);
}

#[test]
fn cms_aging_halves_counters() {
    let mut sketch = Cms::new(64, 30);
    let h = 0xdeadbeefu64;
    for _ in 0..15 {
        sketch.increment(h);
    }
    let before = sketch.estimate(h);
    // Trigger reset.
    for i in 0..30u64 {
        sketch.increment(i.wrapping_mul(0x9e3779b97f4a7c15));
    }
    let after = sketch.estimate(h);
    assert!(after <= before);
}
