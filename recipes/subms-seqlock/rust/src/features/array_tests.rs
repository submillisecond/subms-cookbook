use std::thread;

use super::*;

#[test]
fn slots_start_from_initial_values_at_version_zero() {
    let (writer, reader) = seqlock_array(&[1u64, 2, 3]);
    assert_eq!(reader.len(), 3);
    assert!(!reader.is_empty());
    assert_eq!(writer.len(), 3);
    assert!(!writer.is_empty());
    for i in 0..3 {
        assert_eq!(reader.read(i), (i + 1) as u64);
        assert_eq!(reader.version(i), 0);
        assert_eq!(writer.version(i), 0);
        assert_eq!(writer.get(i), (i + 1) as u64);
    }
}

#[test]
fn a_write_to_one_slot_leaves_the_others_untouched() {
    let (mut writer, reader) = seqlock_array(&[[0u64; 2]; 4]);
    writer.write(2, |v| v[0] = 9);
    writer.set(2, [9, 9]);
    assert_eq!(reader.read_versioned(2), ([9, 9], 2));
    for i in [0, 1, 3] {
        assert_eq!(reader.read_versioned(i), ([0, 0], 0));
    }
}

#[test]
fn try_read_variants_and_cloned_readers() {
    let (mut writer, reader) = seqlock_array(&[5u64, 6]);
    let other = reader.clone();
    let third = writer.reader();
    writer.set(1, 60);
    assert_eq!(other.try_read(1, 1), Some(60));
    assert_eq!(third.try_read_once(1), Some((60, 1)));
    assert_eq!(reader.try_read(0, 0), None);
}

#[test]
fn read_all_returns_every_slot_in_order() {
    let (mut writer, reader) = seqlock_array(&[0u64; 5]);
    for i in 0..5 {
        writer.set(i, i as u64 * 10);
    }
    let mut out = vec![99u64; 2];
    reader.read_all(&mut out);
    assert_eq!(out, vec![0, 10, 20, 30, 40]);
}

#[test]
#[should_panic(expected = "at least one slot")]
fn an_empty_array_is_refused() {
    let _ = seqlock_array::<u64>(&[]);
}

#[test]
fn stress_two_readers_on_distinct_slots_see_only_their_own_writes() {
    const UPDATES: u64 = 200_000;
    let (mut writer, reader) = seqlock_array(&[[0u64; 4]; 8]);
    let handles: Vec<_> = [1usize, 6]
        .into_iter()
        .map(|slot| {
            let r = reader.clone();
            thread::spawn(move || {
                let mut last = 0u64;
                while last < UPDATES {
                    let (img, version) = r.read_versioned(slot);
                    assert!(img.iter().all(|w| *w == img[0]), "torn {img:?}");
                    assert_eq!(version, img[0]);
                    assert!(img[0] >= last);
                    last = img[0];
                }
            })
        })
        .collect();
    for i in 1..=UPDATES {
        writer.write(1, |img| img.iter_mut().for_each(|w| *w = i));
        writer.write(6, |img| img.iter_mut().for_each(|w| *w = i));
        if i % 1000 == 0 {
            writer.write(3, |img| img[0] = i);
        }
    }
    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(reader.version(0), 0);
    assert_eq!(reader.version(1), UPDATES);
    assert_eq!(reader.version(3), UPDATES / 1000);
}
