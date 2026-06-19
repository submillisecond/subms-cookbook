# subms-events-saga

A [submillisecond.com](https://www.submillisecond.com/cookbook/recipes/subms-events-saga)
cookbook recipe - `concurrency`.

An in-process compensating-step (saga) executor on
[subms-events](https://www.submillisecond.com/cookbook/recipes/subms-events):
define steps with a forward action + a compensation, run them in order, and roll
back the completed steps in reverse on the first failure. Step lifecycle events
flow through subms-events. Reference languages: Rust, Python, Java (byte-identical
SagaReport JSON across all three).

- **Rust:** `cargo add subms-events-saga`
- **Python:** `pip install subms-events-saga`
- **Java:** `com.submillisecond.recipes:subms-events-saga`

This is the in-process executor, not a workflow engine: durability (crash-resume)
and distribution (remote steps, retries, timeouts) are out of scope and not
claimed. Pair with
[subms-ts-wal](https://www.submillisecond.com/cookbook/recipes/subms-ts-wal) to
persist the step log.

Licensed under MIT OR Apache-2.0.
