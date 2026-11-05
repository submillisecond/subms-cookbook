use std::sync::atomic::Ordering;
use std::thread;

use super::*;
use crate::seqlock;

#[test]
fn idle_reads_count_with_no_retries() {
    let (mut writer, reader) = seqlock(1u64);
    let counted = InstrumentedReader::new(reader);
    for i in 0..100u64 {
        writer.set(i);
        assert_eq!(counted.read(), i);
    }
    let s = counted.snapshot();
    assert_eq!(s.reads, 100);
    assert_eq!(s.retries, 0);
    assert_eq!(s.max_retries, 0);
    assert_eq!(s.retry_rate(), 0.0);
    assert_eq!(counted.read_versioned(), (99, 100));
    assert_eq!(counted.try_read(1), Some(99));
    assert_eq!(counted.snapshot().reads, 102);
    assert_eq!(counted.inner().version(), 100);
}

#[test]
fn a_write_in_flight_is_counted_as_retries() {
    let (writer, reader) = seqlock(0u64);
    let counted = InstrumentedReader::new(reader);
    let cell = &writer.inner;
    cell.seq.0.store(1, Ordering::Relaxed);
    assert_eq!(counted.try_read(5), None);
    let s = counted.snapshot();
    assert_eq!((s.reads, s.retries, s.max_retries), (0, 5, 5));
    cell.seq.0.store(2, Ordering::Release);
    assert_eq!(counted.try_read(5), Some(0));
    assert_eq!(counted.snapshot().reads, 1);
    assert_eq!(counted.snapshot().retries, 5);
    cell.seq.0.store(3, Ordering::Relaxed);
    let r = counted.clone();
    let h = thread::spawn(move || r.read());
    thread::sleep(std::time::Duration::from_millis(20));
    cell.words[0].store(7, Ordering::Relaxed);
    cell.seq.0.store(4, Ordering::Release);
    assert_eq!(h.join().unwrap(), 7);
    let s = counted.snapshot();
    assert_eq!(s.reads, 2);
    assert!(s.retries > 5);
    assert!(s.max_retries >= 5);
    assert!(s.retry_rate() > 2.0);
}

#[test]
fn shared_stats_and_reset() {
    let (mut writer, reader) = seqlock(0u64);
    let stats = Arc::new(ReadStats::default());
    let a = InstrumentedReader::with_stats(reader.clone(), stats.clone());
    let b = InstrumentedReader::with_stats(reader, stats.clone());
    writer.set(1);
    a.read();
    b.read();
    b.read();
    assert_eq!(stats.snapshot().reads, 3);
    assert!(Arc::ptr_eq(a.stats(), &stats));
    stats.reset();
    assert_eq!(
        stats.snapshot(),
        ReadStatsSnapshot {
            reads: 0,
            retries: 0,
            max_retries: 0
        }
    );
    assert_eq!(
        ReadStatsSnapshot {
            reads: 0,
            retries: 0,
            max_retries: 0
        }
        .retry_rate(),
        0.0
    );
}

#[test]
fn stress_counted_reads_under_a_live_writer_stay_consistent() {
    const UPDATES: u64 = 200_000;
    let (mut writer, reader) = seqlock([0u64; 4]);
    let counted = InstrumentedReader::new(reader);
    let r = counted.clone();
    let h = thread::spawn(move || {
        let mut last = 0u64;
        while last < UPDATES {
            let img = r.read();
            assert!(img.iter().all(|w| *w == img[0]));
            last = img[0];
        }
    });
    for i in 1..=UPDATES {
        writer.write(|img| img.iter_mut().for_each(|w| *w = i));
    }
    h.join().unwrap();
    let s = counted.snapshot();
    assert!(s.reads >= 1);
    assert!(s.retry_rate() >= 0.0);
}
