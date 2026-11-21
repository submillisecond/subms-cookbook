//! The Linux calls the process stages need, declared against the C library
//! std already links. Five functions do not justify a dependency, and the
//! recipes are zero-dep by rule.

use std::ffi::{c_int, c_long, c_void};
use std::io;
use std::sync::atomic::AtomicU32;

const PROT_READ: c_int = 0x1;
const PROT_WRITE: c_int = 0x2;
const MAP_SHARED: c_int = 0x01;
const MAP_ANONYMOUS: c_int = 0x20;
const FUTEX_WAIT: c_int = 0;
const FUTEX_WAKE: c_int = 1;

#[cfg(target_arch = "x86_64")]
const SYS_FUTEX: c_long = 202;
#[cfg(target_arch = "aarch64")]
const SYS_FUTEX: c_long = 98;

/// 1024 CPUs, the glibc `cpu_set_t` size.
pub(crate) const CPU_WORDS: usize = 16;
pub(crate) type CpuSet = [u64; CPU_WORDS];

unsafe extern "C" {
    fn mmap(
        addr: *mut c_void,
        len: usize,
        prot: c_int,
        flags: c_int,
        fd: c_int,
        off: i64,
    ) -> *mut c_void;
    fn munmap(addr: *mut c_void, len: usize) -> c_int;
    fn fork() -> c_int;
    fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    fn _exit(code: c_int) -> !;
    fn syscall(num: c_long, ...) -> c_long;
    fn sched_getaffinity(pid: c_int, size: usize, mask: *mut u64) -> c_int;
    fn sched_setaffinity(pid: c_int, size: usize, mask: *const u64) -> c_int;
}

/// Anonymous shared mapping of `len` zeroed bytes. Survives `fork`, so parent
/// and child see the same physical pages.
pub(crate) fn map_shared(len: usize) -> io::Result<*mut u8> {
    let p = unsafe {
        mmap(
            std::ptr::null_mut(),
            len,
            PROT_READ | PROT_WRITE,
            MAP_SHARED | MAP_ANONYMOUS,
            -1,
            0,
        )
    };
    if p as isize == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(p.cast())
}

/// # Safety
/// `p` and `len` must come from one `map_shared` call, with nothing still
/// referencing the mapping.
pub(crate) unsafe fn unmap(p: *mut u8, len: usize) {
    unsafe { munmap(p.cast(), len) };
}

pub(crate) enum Forked {
    Child,
    Parent(c_int),
}

/// # Safety
/// The child must not allocate, lock, or unwind: in a multi-threaded parent
/// another thread may hold the allocator lock at the instant of the fork, and
/// the child inherits it held. End the child with [`exit_now`].
pub(crate) unsafe fn fork_process() -> io::Result<Forked> {
    match unsafe { fork() } {
        -1 => Err(io::Error::last_os_error()),
        0 => Ok(Forked::Child),
        pid => Ok(Forked::Parent(pid)),
    }
}

pub(crate) fn wait_child(pid: c_int) -> io::Result<i32> {
    let mut status: c_int = 0;
    if unsafe { waitpid(pid, &mut status, 0) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(status)
}

/// Exit without running destructors or atexit handlers, which belong to the
/// parent.
pub(crate) fn exit_now(code: i32) -> ! {
    unsafe { _exit(code) }
}

/// Sleep while `*word == expected`. Returns on a wake, a signal, or at once
/// if the value has already moved; callers recheck either way.
pub(crate) fn futex_wait(word: &AtomicU32, expected: u32) {
    unsafe {
        syscall(
            SYS_FUTEX,
            word.as_ptr(),
            FUTEX_WAIT,
            expected,
            std::ptr::null::<c_void>(),
        );
    }
}

pub(crate) fn futex_wake(word: &AtomicU32) {
    unsafe {
        syscall(SYS_FUTEX, word.as_ptr(), FUTEX_WAKE, 1 as c_int);
    }
}

/// The calling thread's allowed CPUs.
pub(crate) fn get_affinity() -> Option<CpuSet> {
    let mut set: CpuSet = [0; CPU_WORDS];
    let rc = unsafe { sched_getaffinity(0, std::mem::size_of::<CpuSet>(), set.as_mut_ptr()) };
    (rc == 0).then_some(set)
}

/// Restrict the calling thread (not the process) to `set`.
pub(crate) fn set_affinity(set: &CpuSet) -> bool {
    unsafe { sched_setaffinity(0, std::mem::size_of::<CpuSet>(), set.as_ptr()) == 0 }
}
