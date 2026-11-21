//! One 64-byte message, handed from one end to the other and back, across
//! every boundary a trading system usually crosses: two threads, two
//! processes over shared memory, a Unix socket, TCP loopback, and a
//! server-sent-events stream over TCP.
//!
//! The reusable parts are the shared-memory [`ring::Endpoint`] (the same ring
//! code runs between threads on the heap or between processes in a shared
//! mapping) and the [`sse`] writer and reader. The [`handoff`] module holds
//! the ping-pong drivers the bench uses; each returns one-way latency samples
//! computed as round trip / 2.
//!
//! ```
//! use subms_ipc_handoff::ring::{Endpoint, MSG_LEN};
//!
//! let (mut a, mut b) = Endpoint::heap_pair();
//! let msg = [7u8; MSG_LEN];
//! assert!(a.try_send(&msg));
//! let mut got = [0u8; MSG_LEN];
//! assert!(b.try_recv(&mut got));
//! assert_eq!(got, msg);
//! ```
//!
//! Full writeup, design notes and measured benchmarks:
//! <https://www.submillisecond.com/cookbook/recipes/subms-ipc-handoff>

pub mod handoff;
pub mod pin;
pub mod ring;
pub mod sse;

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
pub(crate) mod sys;

#[cfg(feature = "harness")]
pub mod recipe;
