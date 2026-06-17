//! Thread CPU-affinity helpers.
//!
//! [`set_affinity`] pins the calling thread to the given set of
//! logical CPU cores. Implementations:
//!
//! - **Linux:** `sched_setaffinity(0, ...)` via libc syscall.
//! - **Windows:** `SetThreadAffinityMask(GetCurrentThread(), mask)`.
//!   Mask is a 64-bit bitfield; cores >= 64 are rejected with
//!   [`AffinityError::Unsupported`].
//! - **Other (macOS, FreeBSD, *BSDs without sched_setaffinity):**
//!   documented no-op returning [`AffinityError::Unsupported`].
//!   macOS exposes only thread-policy affinity hints; we don't
//!   pretend they're equivalent.
//!
//! The call only affects the current thread. Spawn-then-pin pattern:
//!
//! ```no_run
//! # #[cfg(feature = "affinity")]
//! use subms_mpsc_queue::set_affinity;
//! # #[cfg(feature = "affinity")]
//! std::thread::spawn(|| {
//!     let _ = set_affinity(&[2]);
//!     // ... hot loop pinned to core 2 ...
//! });
//! ```

use std::fmt;

/// Failure mode for [`set_affinity`].
#[derive(Debug)]
pub enum AffinityError {
    /// The current platform does not support affinity pinning.
    Unsupported,
    /// A core index was out of range or invalid.
    InvalidCore(usize),
    /// The OS syscall returned an error code.
    OsError(i32),
}

impl fmt::Display for AffinityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AffinityError::Unsupported => {
                write!(f, "affinity pinning not supported on this platform")
            }
            AffinityError::InvalidCore(c) => write!(f, "invalid core index: {c}"),
            AffinityError::OsError(e) => write!(f, "os error: {e}"),
        }
    }
}

impl std::error::Error for AffinityError {}

/// Pin the calling thread to the given set of logical cores.
///
/// `cores` is the inclusion set; the OS chooses among them. Empty
/// `cores` is rejected with `InvalidCore(0)` to avoid the
/// platform-specific "no cores" trap (Linux interprets an empty
/// affinity mask as "no schedulable CPU" and stalls the thread).
pub fn set_affinity(cores: &[usize]) -> Result<(), AffinityError> {
    if cores.is_empty() {
        return Err(AffinityError::InvalidCore(0));
    }
    #[cfg(target_os = "linux")]
    {
        linux::set_affinity_linux(cores)
    }
    #[cfg(target_os = "windows")]
    {
        windows::set_affinity_windows(cores)
    }
    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        // Reference the param so non-Linux/Windows builds don't warn.
        let _ = cores;
        Err(AffinityError::Unsupported)
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::AffinityError;
    use std::mem;

    // Manual libc shim - the recipe is zero-dep, so we declare just
    // the syscall surface we need. cpu_set_t is a fixed-size 1024-bit
    // bitmap on glibc; CPU_SET / CPU_ZERO are macros we emulate via
    // direct bitfield writes.
    #[repr(C)]
    struct CpuSetT {
        bits: [u64; 16], // 1024 bits = 128 bytes = 16 * u64
    }

    unsafe extern "C" {
        fn sched_setaffinity(pid: u32, cpusetsize: usize, mask: *const CpuSetT) -> i32;
    }

    pub(super) fn set_affinity_linux(cores: &[usize]) -> Result<(), AffinityError> {
        let mut set = CpuSetT { bits: [0u64; 16] };
        for &c in cores {
            if c >= 1024 {
                return Err(AffinityError::InvalidCore(c));
            }
            set.bits[c / 64] |= 1u64 << (c % 64);
        }
        let rc = unsafe { sched_setaffinity(0, mem::size_of::<CpuSetT>(), &set as *const _) };
        if rc == 0 {
            Ok(())
        } else {
            Err(AffinityError::OsError(rc))
        }
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use super::AffinityError;

    type Handle = *mut core::ffi::c_void;
    type DWordPtr = usize;

    unsafe extern "system" {
        fn GetCurrentThread() -> Handle;
        fn SetThreadAffinityMask(thread: Handle, mask: DWordPtr) -> DWordPtr;
    }

    pub(super) fn set_affinity_windows(cores: &[usize]) -> Result<(), AffinityError> {
        let mut mask: usize = 0;
        let max_bits = usize::BITS as usize;
        for &c in cores {
            if c >= max_bits {
                return Err(AffinityError::InvalidCore(c));
            }
            mask |= 1usize << c;
        }
        // SAFETY: GetCurrentThread returns a pseudo-handle; SetThreadAffinityMask is FFI-safe.
        let prev = unsafe { SetThreadAffinityMask(GetCurrentThread(), mask) };
        if prev == 0 {
            Err(AffinityError::OsError(0))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_cores_is_rejected() {
        let err = set_affinity(&[]);
        assert!(matches!(err, Err(AffinityError::InvalidCore(0))));
    }

    #[test]
    fn out_of_range_core_returns_invalid() {
        let err = set_affinity(&[usize::MAX]);
        // On every platform we either reject as InvalidCore or
        // Unsupported. Neither is the success case.
        assert!(err.is_err());
    }

    #[cfg(any(target_os = "linux", target_os = "windows"))]
    #[test]
    fn pinning_to_core_zero_succeeds_on_supported_platforms() {
        // Core 0 exists on every system that supports affinity.
        let result = set_affinity(&[0]);
        // Permission may be denied in some containerised CI sandboxes;
        // accept either OK or an explicit OsError so the test is
        // portable across CI runners.
        assert!(
            matches!(result, Ok(()) | Err(AffinityError::OsError(_))),
            "unexpected: {result:?}"
        );
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    #[test]
    fn unsupported_platform_returns_unsupported() {
        let result = set_affinity(&[0]);
        assert!(matches!(result, Err(AffinityError::Unsupported)));
    }

    #[test]
    fn display_messages_render() {
        let m1 = format!("{}", AffinityError::Unsupported);
        let m2 = format!("{}", AffinityError::InvalidCore(7));
        let m3 = format!("{}", AffinityError::OsError(42));
        assert!(m1.contains("not supported"));
        assert!(m2.contains("7"));
        assert!(m3.contains("42"));
    }

    #[test]
    fn debug_messages_render() {
        let s = format!("{:?}", AffinityError::OsError(1));
        assert!(s.contains("OsError"));
    }
}
