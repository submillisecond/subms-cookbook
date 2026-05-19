use std::thread;
use subms_spsc_ring_buffer::SpscRingBuffer;

fn main() {
    let (mut tx, mut rx) = SpscRingBuffer::with_capacity::<u32>(16);

    let producer = thread::spawn(move || {
        for i in 0..10u32 {
            while tx.try_push(i).is_err() {}
        }
    });

    let consumer = thread::spawn(move || {
        let mut seen = Vec::new();
        while seen.len() < 10 {
            if let Some(v) = rx.try_pop() {
                seen.push(v);
            }
        }
        seen
    });

    producer.join().unwrap();
    let seen = consumer.join().unwrap();
    println!("consumed: {seen:?}");
}
