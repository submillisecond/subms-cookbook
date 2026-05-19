# subms-spsc-ring-buffer (Rust)

Wait-free single-producer single-consumer ring buffer.

## Sizing

Power-of-two capacity (rounded up from the request). Index modulo via bitmask, not `%`. Per-side counter padding to 128 bytes to avoid false sharing across the head/tail line.

## Implementation notes

- Producer caches the consumer head; consumer caches the producer tail. Re-read through the atomic only when the cache says full / empty.
- Memory ordering: `Release` on publish (tail/head store), `Acquire` on the opposite-side load, `Relaxed` on own-side reads.
- `Producer`/`Consumer` split enforces SPSC at the type level - moving the wrong handle to the wrong thread is a compile error.

## Quality bar

- Reference impl: Java JCTools `SpscArrayQueue`, Agrona `OneToOneConcurrentArrayQueue` (Aeron). Rust `rtrb`.
- Sub-ms claim under: enqueue/dequeue p99 < 1 ms across 100k ops with capacity 1024, on a 2-core sibling pair.
- Not claimed: MPMC; multi-core throughput claims on logical (HT) sibling pairs.

## Bench numbers

Filled in once the bench is run on the canonical hardware. The cookbook page carries the measured values - never invented.

## Consumed by

Timer wheel (`subms-timer-wheel`) uses an SPSC ring as the schedule-from-any-thread inbox; the timer thread is the consumer.
