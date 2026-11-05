use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Quote {
    bid: i64,
    ask: i64,
    mid: f64,
}

impl SeqLockImage for Quote {
    const WORDS: usize = 3;
    fn store(&self, mut put: impl FnMut(usize, u64)) {
        put(0, self.bid as u64);
        put(1, self.ask as u64);
        put(2, self.mid.to_bits());
    }
    fn load(mut get: impl FnMut(usize) -> u64) -> Self {
        Quote {
            bid: get(0) as i64,
            ask: get(1) as i64,
            mid: f64::from_bits(get(2)),
        }
    }
}

#[test]
fn initial_value_reads_back_at_version_zero() {
    let (writer, reader) = seqlock(42u64);
    assert_eq!(reader.read(), 42);
    assert_eq!(reader.version(), 0);
    assert_eq!(writer.version(), 0);
    assert_eq!(writer.get(), 42);
    assert!(!reader.is_writing());
}

#[test]
fn write_closure_edits_the_shadow_and_publishes() {
    let (mut writer, reader) = seqlock([1u64, 2, 3]);
    writer.write(|v| v[1] = 20);
    assert_eq!(reader.read(), [1, 20, 3]);
    writer.write(|v| v[0] += 100);
    assert_eq!(writer.get(), [101, 20, 3]);
    assert_eq!(reader.read_versioned(), ([101, 20, 3], 2));
}

#[test]
fn set_replaces_and_bumps_the_version() {
    let (mut writer, reader) = seqlock(-5i64);
    writer.set(7);
    writer.set(-9);
    assert_eq!(reader.read(), -9);
    assert_eq!(reader.version(), 2);
}

#[test]
fn every_provided_image_type_round_trips() {
    let (mut w, r) = seqlock(f64::NAN);
    assert!(r.read().is_nan());
    w.set(-0.0);
    assert_eq!(r.read().to_bits(), (-0.0f64).to_bits());
    let (mut w, r) = seqlock([-1i64, i64::MIN]);
    w.set([i64::MAX, 0]);
    assert_eq!(r.read(), [i64::MAX, 0]);
    let (mut w, r) = seqlock([1.5f64, -2.5]);
    w.write(|v| v[1] = f64::INFINITY);
    assert_eq!(r.read(), [1.5, f64::INFINITY]);
    let (mut w, r) = seqlock(u64::MAX);
    w.set(0);
    assert_eq!(r.read(), 0);
}

#[test]
fn custom_struct_image_round_trips() {
    let q = Quote {
        bid: 99_995,
        ask: 100_005,
        mid: 100_000.0,
    };
    let (mut writer, reader) = seqlock(q);
    assert_eq!(reader.read(), q);
    writer.write(|v| {
        v.bid = -1;
        v.mid = 0.5;
    });
    assert_eq!(
        reader.read(),
        Quote {
            bid: -1,
            ask: 100_005,
            mid: 0.5
        }
    );
}

#[test]
fn readers_clone_and_share_one_lock() {
    let (mut writer, reader) = seqlock(0u64);
    let other = reader.clone();
    let third = writer.reader();
    writer.set(9);
    assert_eq!(reader.read(), 9);
    assert_eq!(other.read(), 9);
    assert_eq!(third.read(), 9);
    assert_eq!(third.version(), 1);
}

#[test]
fn try_read_succeeds_on_an_idle_lock_and_try_read_once_reports_version() {
    let (mut writer, reader) = seqlock(3u64);
    assert_eq!(reader.try_read(1), Some(3));
    writer.set(4);
    assert_eq!(reader.try_read_once(), Some((4, 1)));
    assert_eq!(reader.try_read(0), None);
}

#[test]
fn torn_read_is_rejected_by_the_sequence() {
    let (writer, reader) = seqlock([1u64, 1]);
    let cell = &writer.inner;
    let s = cell.seq.0.load(Ordering::Relaxed);
    cell.seq.0.store(s + 1, Ordering::Relaxed);
    assert!(reader.is_writing());
    assert_eq!(reader.try_read_once(), None);
    assert_eq!(reader.try_read(3), None);
    cell.words[0].store(2, Ordering::Relaxed);
    cell.seq.0.store(s + 2, Ordering::Release);
    assert_eq!(reader.read(), [2, 1]);
    assert_eq!(reader.version(), 1);
}

#[test]
#[should_panic(expected = "at least one word")]
fn an_empty_image_is_refused() {
    let _ = seqlock([0u64; 0]);
}

#[test]
fn a_wide_image_round_trips() {
    let (mut w, r) = seqlock([7u64; 512]);
    w.write(|v| v[511] = 1);
    let got = r.read();
    assert_eq!(got[0], 7);
    assert_eq!(got[511], 1);
}

#[test]
fn version_survives_sequence_wraparound_arithmetic() {
    let (mut writer, reader) = seqlock(0u64);
    writer.inner.seq.0.store(u64::MAX - 1, Ordering::Relaxed);
    writer.set(1);
    assert_eq!(reader.read(), 1);
    assert_eq!(reader.version(), 0);
    assert!(!reader.is_writing());
}

#[test]
fn stress_four_readers_never_observe_a_torn_image() {
    const UPDATES: u64 = 300_000;
    let (mut writer, reader) = seqlock([0u64; 8]);
    let stop = Arc::new(AtomicBool::new(false));

    let readers: Vec<_> = (0..4)
        .map(|_| {
            let r = reader.clone();
            let stop = stop.clone();
            thread::spawn(move || {
                let mut reads = 0u64;
                let mut last = 0u64;
                while !stop.load(Ordering::Relaxed) || last < UPDATES {
                    let (img, version) = r.read_versioned();
                    for w in &img {
                        assert_eq!(*w, img[0], "torn image {img:?}");
                    }
                    assert_eq!(version, img[0], "version does not match the write");
                    assert!(img[0] >= last, "went backwards");
                    last = img[0];
                    reads += 1;
                    if last >= UPDATES {
                        break;
                    }
                }
                reads
            })
        })
        .collect();

    for i in 1..=UPDATES {
        writer.write(|img| img.iter_mut().for_each(|w| *w = i));
    }
    stop.store(true, Ordering::Relaxed);
    let total: u64 = readers.into_iter().map(|h| h.join().unwrap()).sum();
    assert!(total >= 4, "every reader read at least once");
    assert_eq!(reader.read(), [UPDATES; 8]);
    assert_eq!(writer.version(), UPDATES);
}
