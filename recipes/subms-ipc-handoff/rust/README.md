# subms-ipc-handoff (Rust)

One 64-byte message, round trip / 2, across six boundaries.

## Stages

- `thread-ring-spin` - two threads, a heap ring each way, busy-spin.
- `process-shm-spin` - two processes (`fork`), the same ring in an anonymous `MAP_SHARED` mapping, busy-spin. Linux x86_64 / aarch64.
- `process-shm-futex` - as above, both ends park on a futex and the sender wakes. Linux x86_64 / aarch64.
- `uds-stream` - `UnixStream::pair`, blocking `write_all` / `read_exact`. Unix.
- `tcp-loopback` - 127.0.0.1, `TCP_NODELAY` both ends.
- `sse-loopback` - HTTP/1.1 `text/event-stream` over loopback; the server writes each message as one event, the client parses it and returns a raw 64-byte ack.

A stage the platform cannot run is skipped and named in the `stages_skipped` meta. `peer_cpus` records the core pair the two ends ran on; `unpinned` means they were not pinned and the capture is not valid.

## Implementation notes

- One `#[repr(C)]` ring serves the thread and process stages; zero bytes are a valid empty ring, so an anonymous mapping needs no initialisation.
- The park protocol is a Dekker pair over `waiting` and the ring tail with a `SeqCst` fence on each side, plus a futex on a sequence word so a wake that lands before the sleep is not lost.
- The forked echo never allocates or unwinds, and exits with `_exit`.

## Quality bar

- Reference points: Aeron IPC and Chronicle Queue (shared-memory rings between processes), the Linux `futex(2)` wait/wake contract, the WHATWG server-sent-events parsing rules.
- Sub-ms claim under: `thread-ring-spin` and `process-shm-spin` p99 < 1 ms on two isolated physical cores. The socket and SSE stages are measured, not budgeted.
- Not claimed: TLS, chunked transfer encoding, a reverse proxy, a browser's event loop, cross-host latency, kernel bypass, throughput.

## Bench numbers

Filled in once the bench is run on the canonical hardware. The cookbook page carries the measured values - never invented.
