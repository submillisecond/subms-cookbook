use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::SpscRingBuffer;

#[test]
fn busy_spin_pushes_and_pops_in_a_single_thread() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let mut p = BlockingSpscProducer::new(tx, BusySpin);
    let mut c = BlockingSpscConsumer::new(rx, BusySpin);
    p.push(7);
    assert_eq!(c.pop(), 7);
}

#[test]
fn busy_spin_try_push_and_try_pop_capacity() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let mut p = BlockingSpscProducer::new(tx, BusySpin);
    let mut c = BlockingSpscConsumer::new(rx, BusySpin);
    assert_eq!(p.capacity(), 4);
    assert_eq!(c.capacity(), 4);
    assert!(p.try_push(1).is_ok());
    assert_eq!(c.try_pop(), Some(1));
    assert_eq!(c.try_pop(), None);
}

#[test]
fn yield_strategy_handles_full_then_drains() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let mut p = BlockingSpscProducer::new(tx, YieldStrategy);
    let mut c = BlockingSpscConsumer::new(rx, YieldStrategy);

    let consumer = thread::spawn(move || {
        let mut v = Vec::new();
        for _ in 0..20 {
            v.push(c.pop());
        }
        v
    });
    for i in 0..20u32 {
        p.push(i);
    }
    let received = consumer.join().unwrap();
    for (i, v) in received.iter().enumerate() {
        assert_eq!(*v, i as u32);
    }
}

#[test]
fn park_strategy_wakes_blocked_consumer() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let (p_strat, c_strat) = ParkStrategy::pair();
    let mut p = BlockingSpscProducer::new(tx, p_strat);
    let mut c = BlockingSpscConsumer::new(rx, c_strat);

    let started = Instant::now();
    let consumer = thread::spawn(move || c.pop());

    // Make sure the consumer is parked before we push.
    thread::sleep(Duration::from_millis(20));
    p.push(42);

    let v = consumer.join().unwrap();
    assert_eq!(v, 42);
    // Bounded so a missed unpark fails fast rather than hanging.
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "consumer didn't wake within 2s"
    );
}

#[test]
fn park_strategy_wakes_blocked_producer() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(2);
    let (p_strat, c_strat) = ParkStrategy::pair();
    let mut p = BlockingSpscProducer::new(tx, p_strat);
    let mut c = BlockingSpscConsumer::new(rx, c_strat);

    // Fill the ring before spawning so the producer thread will block.
    p.push(1);
    p.push(2);

    let started = Instant::now();
    let producer = thread::spawn(move || {
        p.push(3);
        p.push(4);
    });

    thread::sleep(Duration::from_millis(20));
    assert_eq!(c.pop(), 1);
    assert_eq!(c.pop(), 2);
    producer.join().unwrap();
    assert_eq!(c.pop(), 3);
    assert_eq!(c.pop(), 4);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "producer didn't unblock within 2s"
    );
}

// Modest colocated throughput check; the larger park round-trip lives in
// tests/stress.rs so the coverage --lib run stays fast.
#[test]
fn park_strategy_handles_high_throughput_round_trip() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u64>(64);
    let (p_strat, c_strat) = ParkStrategy::pair();
    let mut p = BlockingSpscProducer::new(tx, p_strat);
    let mut c = BlockingSpscConsumer::new(rx, c_strat);

    let n = 5_000u64;
    let producer = thread::spawn(move || {
        for i in 0..n {
            p.push(i);
        }
    });
    let consumer = thread::spawn(move || {
        for i in 0..n {
            assert_eq!(c.pop(), i);
        }
    });
    producer.join().unwrap();
    consumer.join().unwrap();
}

#[test]
fn try_push_succeeds_without_blocking_when_slot_free() {
    let (tx, _rx) = SpscRingBuffer::with_capacity::<u32>(4);
    let mut p = BlockingSpscProducer::new(tx, BusySpin);
    assert!(p.try_push(1).is_ok());
    assert!(p.try_push(2).is_ok());
}

#[test]
fn strategy_wait_and_signal_are_directly_callable() {
    // The single-thread blocking tests never actually back off, so the
    // BusySpin / YieldStrategy `wait` bodies are only reachable by driving
    // the trait method directly.
    let mut busy = BusySpin;
    busy.wait();
    busy.signal();

    let mut yielder = YieldStrategy;
    yielder.wait();
    yielder.signal();
}

#[test]
fn park_strategy_signal_on_main_thread_runs_unpark_path() {
    // Drive both unpark paths on the main thread (no thread registered, so
    // each is a no-op) to exercise the internal mutex acquire + Option::take
    // without depending on child-thread scheduling.
    let (p_strat, c_strat) = ParkStrategy::pair();
    p_strat.signal();
    c_strat.signal();
}

#[test]
fn busy_spin_blocking_push_then_pop_full_cycle() {
    let (tx, rx) = SpscRingBuffer::with_capacity::<u32>(2);
    let mut p = BlockingSpscProducer::new(tx, BusySpin);
    let mut c = BlockingSpscConsumer::new(rx, BusySpin);
    p.push(1);
    p.push(2);
    assert_eq!(c.pop(), 1);
    assert_eq!(c.pop(), 2);
    assert_eq!(c.try_pop(), None);
}
