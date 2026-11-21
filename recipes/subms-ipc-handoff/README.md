# IPC handoff

One 64-byte message handed across a thread ring, a shared-memory ring between two processes (spinning and futex-parked), a Unix socket, TCP loopback and a server-sent-events stream. Every stage reports round trip / 2 with both ends pinned to separate cores.

Part of the [submillisecond.com cookbook](https://www.submillisecond.com/cookbook/recipes/subms-ipc-handoff) - a `concurrency` recipe. Rust first; the Java port follows. Zero runtime dependencies (`std` only; the Linux calls are declared against the C library std already links, and the perf harness is an opt-in Cargo feature).

## Install

```toml
# Cargo.toml
subms-ipc-handoff = "0.10"
```

## Docs, design, and measured benchmarks

The full writeup - what each stage crosses, why round trip / 2, the quality-bar
contract (reference points / claim conditions / non-claims), and the numbers
captured on the stated hardware - lives at:

**https://www.submillisecond.com/cookbook/recipes/subms-ipc-handoff**

## Layout

- [`rust/`](./rust/) - Rust edition 2024, `std`-only library (crate `subms_ipc_handoff`). The shared-memory stages build on Linux x86_64 and aarch64.
- [`.subms/`](./.subms/) - capture controls; the fleet-captured perf JSON lands here.

## License

Dual-licensed under [MIT](../../LICENSE-MIT) OR [Apache-2.0](../../LICENSE-APACHE), at your option.
