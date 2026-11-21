//! Put the two ends of a handoff on two different cores. A ping-pong whose
//! ends share a core measures the scheduler swapping them, not the transport.
//!
//! Linux only; elsewhere every call is a no-op and [`peer_cpus`] is `None`, so
//! a capture taken there says it ran unpinned.

/// The first two CPUs the calling thread may run on: `(initiator, echo)`.
/// `None` when fewer than two are allowed or the platform cannot pin.
pub fn peer_cpus() -> Option<(usize, usize)> {
    imp::allowed().and_then(|cpus| match cpus.as_slice() {
        [a, b, ..] => Some((*a, *b)),
        _ => None,
    })
}

/// Pins the calling thread to one CPU and restores its previous mask on drop,
/// so a stage does not leave the harness thread pinned for the next one.
pub struct PinGuard {
    restore: Option<imp::Saved>,
}

impl PinGuard {
    /// Pin to `cpu`, or do nothing when `cpu` is `None`.
    pub fn pin(cpu: Option<usize>) -> PinGuard {
        PinGuard {
            restore: cpu.and_then(imp::pin_saving),
        }
    }

    /// Whether this guard actually pinned the thread.
    pub fn pinned(&self) -> bool {
        self.restore.is_some()
    }
}

impl Drop for PinGuard {
    fn drop(&mut self) {
        if let Some(s) = self.restore.take() {
            imp::restore(s);
        }
    }
}

#[cfg(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
))]
mod imp {
    use crate::sys::{CPU_WORDS, CpuSet, get_affinity, set_affinity};

    pub(super) type Saved = CpuSet;

    pub(super) fn allowed() -> Option<Vec<usize>> {
        let set = get_affinity()?;
        Some(
            (0..CPU_WORDS * 64)
                .filter(|c| set[c / 64] & (1u64 << (c % 64)) != 0)
                .collect(),
        )
    }

    pub(super) fn pin_saving(cpu: usize) -> Option<Saved> {
        if cpu >= CPU_WORDS * 64 {
            return None;
        }
        let saved = get_affinity()?;
        let mut one: CpuSet = [0; CPU_WORDS];
        one[cpu / 64] = 1u64 << (cpu % 64);
        set_affinity(&one).then_some(saved)
    }

    pub(super) fn restore(s: Saved) {
        set_affinity(&s);
    }
}

#[cfg(not(all(
    target_os = "linux",
    any(target_arch = "x86_64", target_arch = "aarch64")
)))]
mod imp {
    pub(super) type Saved = ();

    pub(super) fn allowed() -> Option<Vec<usize>> {
        None
    }

    pub(super) fn pin_saving(_cpu: usize) -> Option<Saved> {
        None
    }

    pub(super) fn restore(_s: Saved) {}
}

#[cfg(test)]
#[path = "pin_tests.rs"]
mod tests;
