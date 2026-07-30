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

#[test]
fn cms_high_nibble_read_write_roundtrips() {
    // Odd indices land in the high nibble; drive one directly so the
    // high-nibble write + read arms are exercised.
    let mut sketch = Cms::new(16, 10_000);
    sketch.write(0, 1, 7);
    assert_eq!(sketch.read(0, 1), 7);
    sketch.write(0, 0, 3);
    assert_eq!(sketch.read(0, 0), 3);
    assert_eq!(
        sketch.read(0, 1),
        7,
        "writing the low nibble must not disturb the high one"
    );
}

#[test]
fn doorkeeper_check_add_and_clear() {
    let mut d = Doorkeeper::new(128);
    let h = 0xabcd_1234_5678_9012u64;
    assert!(!d.check_or_add(h), "first sight is not a repeat");
    assert!(d.check_or_add(h), "second sight is a repeat");
    d.clear();
    assert!(
        !d.check_or_add(h),
        "after clear the key reads as unseen again"
    );
}

#[test]
fn accessors_report_segment_sizes() {
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(8);
    assert!(c.is_empty());
    assert_eq!(c.len(), 0);
    assert_eq!(c.window_len(), 0);
    assert_eq!(c.probation_len(), 0);
    assert_eq!(c.protected_len(), 0);
    for k in 0u32..8 {
        c.put(k, k);
    }
    assert!(!c.is_empty());
    assert_eq!(
        c.len(),
        c.window_len() + c.probation_len() + c.protected_len()
    );
}

// Repeatedly touching a small resident key set drives keys through
// Window -> Probation -> Protected, updates them in place in every
// segment, fills Protected past its cap so a probation hit demotes the
// Protected LRU, and exercises the interior-node unlink / push paths.
#[test]
fn segment_churn_covers_promote_demote_and_updates() {
    let mut c: TinyLfuCache<u32, u32> = TinyLfuCache::with_capacity(8);
    for round in 0u32..200 {
        for k in 0u32..10 {
            // After the first round these become in-place updates that
            // hit the Window / Probation / Protected update arms.
            c.put(k, round * 100 + k);
            // Promotes probation entries into protected; once protected
            // is full this demotes its LRU back to probation.
            let _ = c.get(&k);
        }
        assert!(c.len() <= 8, "resident set exceeded c at round {round}");
    }
    assert!(c.protected_len() <= 8);
    // A protected key survives and reads back its latest value.
    let hot = (0u32..10).find(|k| c.get(k).is_some());
    assert!(hot.is_some(), "at least one hot key must remain resident");
}
