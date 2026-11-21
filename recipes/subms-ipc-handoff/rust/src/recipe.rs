//! `SubMsRecipe` impl. Behind the `harness` feature.
//!
//! One stage per transport, each `entries` round trips of a 64-byte message.
//! A stage the platform cannot run is skipped and named in the
//! `stages_skipped` meta, so a capture from a laptop cannot pass for one from
//! the fleet.

use std::io;

use subms::{SubMsBenchParams, SubMsPerfHarness, SubMsRecipe, SubMsStageKind};

use crate::handoff::{self, Wait};
use crate::pin::peer_cpus;
use crate::ring::MSG_LEN;

pub struct IpcHandoffRecipe;

type Driver = fn(usize, usize) -> io::Result<Vec<u64>>;

fn stages() -> Vec<(&'static str, Option<Driver>)> {
    #[cfg(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    ))]
    let (shm_spin, shm_park): (Option<Driver>, Option<Driver>) = (
        Some(|n, w| handoff::process_ring(n, w, Wait::Spin)),
        Some(|n, w| handoff::process_ring(n, w, Wait::Park)),
    );
    #[cfg(not(all(
        target_os = "linux",
        any(target_arch = "x86_64", target_arch = "aarch64")
    )))]
    let (shm_spin, shm_park): (Option<Driver>, Option<Driver>) = (None, None);

    #[cfg(unix)]
    let uds: Option<Driver> = Some(handoff::uds_stream);
    #[cfg(not(unix))]
    let uds: Option<Driver> = None;

    vec![
        (
            "thread-ring-spin",
            Some(|n, w| handoff::thread_ring(n, w, Wait::Spin)),
        ),
        ("process-shm-spin", shm_spin),
        ("process-shm-futex", shm_park),
        ("uds-stream", uds),
        ("tcp-loopback", Some(handoff::tcp_loopback)),
        ("sse-loopback", Some(handoff::sse_loopback)),
    ]
}

impl SubMsRecipe for IpcHandoffRecipe {
    fn name(&self) -> &str {
        "ipc-handoff"
    }

    fn run(&self, h: &mut SubMsPerfHarness, params: &SubMsBenchParams) {
        let mut skipped = Vec::new();
        for (name, driver) in stages() {
            let Some(run) = driver else {
                skipped.push(name);
                continue;
            };
            let samples =
                run(params.entries, params.warmup).unwrap_or_else(|e| panic!("stage {name}: {e}"));
            let s = h
                .stage(name, samples.len())
                .with_kind(SubMsStageKind::HotPath);
            for ns in samples {
                s.record(ns);
            }
        }
        let cores = match peer_cpus() {
            Some((a, b)) => format!("{a},{b}"),
            None => "unpinned".to_string(),
        };
        h.add_meta("peer_cpus", &cores);
        h.add_meta("payload_bytes", &MSG_LEN.to_string());
        h.add_meta("sample", "round_trip_ns / 2");
        h.add_meta("sse_return_leg", "raw 64-byte ack on the same connection");
        h.add_meta("proxy", "none");
        h.add_meta("tls", "none");
        if !skipped.is_empty() {
            h.add_meta("stages_skipped", &skipped.join(","));
        }
    }
}
