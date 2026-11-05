use std::sync::atomic::Ordering;
use std::thread;

use super::*;

#[test]
fn initial_value_and_version() {
    let (writer, reader) = seqlatch([1u64, 2]);
    assert_eq!(reader.read(), [1, 2]);
    assert_eq!(reader.version(), 0);
    assert_eq!(writer.version(), 0);
    assert_eq!(writer.get(), [1, 2]);
}

#[test]
fn write_and_set_publish_to_both_copies() {
    let (mut writer, reader) = seqlatch(0u64);
    writer.write(|v| *v = 5);
    assert_eq!(reader.read_versioned(), (5, 1));
    writer.set(6);
    assert_eq!(reader.read_versioned(), (6, 2));
    for copy in &writer.inner.copies {
        assert_eq!(copy[0].load(Ordering::Relaxed), 6);
    }
    assert_eq!(writer.get(), 6);
}

#[test]
fn a_reader_between_the_two_flips_gets_the_previous_value_consistently() {
    let (mut writer, reader) = seqlatch([10u64, 10]);
    writer.set([20, 20]);
    let cell = &writer.inner;
    let s = cell.seq.0.load(Ordering::Relaxed);
    cell.seq.0.store(s + 1, Ordering::Release);
    cell.copies[(s & 1) as usize][0].store(30, Ordering::Relaxed);
    assert_eq!(reader.read_versioned(), ([20, 20], 1));
    assert_eq!(reader.try_read(1), Some([20, 20]));
    cell.copies[(s & 1) as usize][1].store(30, Ordering::Relaxed);
    cell.seq.0.store(s + 2, Ordering::Release);
    assert_eq!(reader.read_versioned(), ([30, 30], 2));
}

#[test]
fn cloned_readers_and_bounded_reads() {
    let (mut writer, reader) = seqlatch(1u64);
    let other = reader.clone();
    let third = writer.reader();
    writer.set(2);
    assert_eq!(other.read(), 2);
    assert_eq!(third.try_read_once(), Some((2, 1)));
    assert_eq!(reader.try_read(0), None);
}

#[test]
fn stress_readers_never_see_a_torn_image_or_a_stale_version_pair() {
    const UPDATES: u64 = 300_000;
    let (mut writer, reader) = seqlatch([0u64; 6]);
    let handles: Vec<_> = (0..3)
        .map(|_| {
            let r = reader.clone();
            thread::spawn(move || {
                let mut last = 0u64;
                let mut reads = 0u64;
                while last < UPDATES {
                    let (img, version) = r.read_versioned();
                    assert!(img.iter().all(|w| *w == img[0]), "torn {img:?}");
                    assert_eq!(version, img[0], "value and version from different writes");
                    assert!(img[0] >= last);
                    last = img[0];
                    reads += 1;
                }
                reads
            })
        })
        .collect();
    for i in 1..=UPDATES {
        writer.write(|img| img.iter_mut().for_each(|w| *w = i));
    }
    let total: u64 = handles.into_iter().map(|h| h.join().unwrap()).sum();
    assert!(total >= 3);
    assert_eq!(reader.read(), [UPDATES; 6]);
    assert_eq!(writer.version(), UPDATES);
}
