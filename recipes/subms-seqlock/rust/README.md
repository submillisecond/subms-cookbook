# subms-seqlock (Rust)

Single-writer, many-reader snapshot publication.

## Shape

`seqlock(image)` returns a writer and a cloneable reader. The image is a fixed run of 64-bit words (`SeqLockImage`): scalars and arrays of `u64` / `i64` / `f64` are provided, a struct implements it by storing its fields in turn. Writers and readers never take a lock; a reader that collides with a write retries.

## Implementation notes

- Sequence word on its own 128-byte line. Odd while a write is in flight, even otherwise; `version()` is the sequence halved.
- Data words are relaxed atomics, so a torn read is a value the sequence check discards rather than undefined behaviour.
- Writer: relaxed store of the odd sequence, release fence, relaxed word stores, release store of the even sequence. Reader: acquire load, relaxed word loads, acquire fence, relaxed load; equal and even means the copy is good.
- Features: `array` (N slots, one sequence each), `latch` (two copies, readers never wait on a write), `stats` (retry counters).

## Quality bar

- Reference impl: Linux `seqlock.h` / `seqcount_latch`, the `seqlock` crate, crossbeam's internal `SeqLock`; JDK `StampedLock.tryOptimisticRead` as the Java cousin.
- Sub-ms claim under: `write` and `read` p99 < 1 ms across 100k ops on a five-word image with one writer and one spinning reader on a 2-core pair.
- Not claimed: multiple writers; readers that block or sleep; a bound on the retry count under a writer that never pauses.

## Bench numbers

Filled in once the bench is run on the canonical hardware. The cookbook page carries the measured values - never invented.

## Consumed by

Nothing yet.
