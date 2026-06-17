//! `StatsBump`: bump arena with runtime counters for observability.
//!
//! Tracks:
//! - `allocations` - total `alloc_*` calls served (excluding refusals).
//! - `bytes_used` - sum of allocation `size` values (excludes padding).
//! - `bytes_wasted` - sum of alignment padding inserted between
//!   allocations.
//! - `peak_bytes` - high-watermark of cursor across this arena's
//!   lifetime.
//! - `chunk_count` - number of distinct chunks the arena has ever
//!   opened (grows monotonically; this struct also auto-grows so the
//!   counter is meaningful).
//!
//! Counters survive `reset()` so a long-running process can read
//! lifetime aggregates. `clear_stats()` zeros them.

use std::alloc::{Layout, alloc, dealloc};
use std::ptr;

use crate::align_up;

/// Snapshot of runtime counters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct BumpStats {
    pub allocations: u64,
    pub bytes_used: u64,
    pub bytes_wasted: u64,
    pub peak_bytes: u64,
    pub chunk_count: u64,
}

/// Bump arena with instrumentation. Auto-grows like `GrowableBump`.
pub struct StatsBump {
    chunks: Vec<Chunk>,
    cursor: usize,
    stats: BumpStats,
}

struct Chunk {
    ptr: *mut u8,
    layout: Layout,
}

impl Drop for Chunk {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

impl StatsBump {
    /// New arena with a 4 KiB initial chunk.
    pub fn new() -> Self {
        Self::with_capacity(4096)
    }

    /// New arena with the requested initial chunk size.
    pub fn with_capacity(initial: usize) -> Self {
        let initial = initial.max(64);
        let layout = Layout::from_size_align(initial, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating first stats chunk");
        Self {
            chunks: vec![Chunk { ptr, layout }],
            cursor: 0,
            stats: BumpStats {
                chunk_count: 1,
                ..Default::default()
            },
        }
    }

    /// Allocate a `Copy` value. Updates the counters.
    pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T {
        let layout = Layout::new::<T>();
        let p = self.alloc_raw(layout);
        unsafe {
            ptr::write(p as *mut T, value);
            &mut *(p as *mut T)
        }
    }

    /// Allocate `layout.size()` bytes aligned to `layout.align()`.
    /// Updates the counters.
    pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let last_size;
        let last_ptr;
        {
            let last = self.chunks.last().expect("at least one chunk");
            last_size = last.layout.size();
            last_ptr = last.ptr;
        }
        let base = last_ptr as usize;
        let aligned = align_up(base + self.cursor, align) - base;
        let waste = aligned - self.cursor;
        let end = aligned + size;
        if end <= last_size {
            self.cursor = end;
            self.bump_counters(size as u64, waste as u64);
            return unsafe { last_ptr.add(aligned) };
        }
        // Grow.
        self.grow(size + align);
        let last_ptr = self.chunks.last().unwrap().ptr;
        let base = last_ptr as usize;
        let aligned = align_up(base, align) - base;
        // Fresh chunk starts at offset 0, so the waste counter only
        // tracks the per-call padding inside the new chunk (typically
        // zero because chunks are 16-byte aligned; non-zero only if
        // the caller asks for >16-byte alignment).
        self.cursor = aligned + size;
        self.bump_counters(size as u64, aligned as u64);
        unsafe { last_ptr.add(aligned) }
    }

    fn bump_counters(&mut self, size: u64, waste: u64) {
        self.stats.allocations += 1;
        self.stats.bytes_used += size;
        self.stats.bytes_wasted += waste;
        let cursor = self.cursor as u64;
        if cursor > self.stats.peak_bytes {
            self.stats.peak_bytes = cursor;
        }
    }

    fn grow(&mut self, min_bytes: usize) {
        let last = self.chunks.last().expect("at least one chunk");
        let new_size = (last.layout.size() * 2).max(min_bytes);
        let layout = Layout::from_size_align(new_size, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM growing arena");
        self.chunks.push(Chunk { ptr, layout });
        self.cursor = 0;
        self.stats.chunk_count += 1;
    }

    /// Rewind. Keeps the largest chunk. Stats are preserved.
    pub fn reset(&mut self) {
        if self.chunks.len() > 1 {
            let largest = self
                .chunks
                .iter()
                .enumerate()
                .max_by_key(|(_, c)| c.layout.size())
                .map(|(i, _)| i)
                .unwrap();
            let keeper = self.chunks.swap_remove(largest);
            self.chunks.clear();
            self.chunks.push(keeper);
        }
        self.cursor = 0;
    }

    /// Snapshot the live counters.
    pub fn stats(&self) -> BumpStats {
        self.stats
    }

    /// Zero the counters. The `chunk_count` is restored to the current
    /// retained chunk count rather than zero, so the snapshot remains
    /// meaningful immediately after.
    pub fn clear_stats(&mut self) {
        self.stats = BumpStats {
            chunk_count: self.chunks.len() as u64,
            ..Default::default()
        };
    }
}

impl Default for StatsBump {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allocations_counter_increments() {
        let mut a = StatsBump::with_capacity(256);
        for i in 0..5u32 {
            let _ = a.alloc_copy(i);
        }
        assert_eq!(a.stats().allocations, 5);
    }

    #[test]
    fn bytes_used_sums_sizes() {
        let mut a = StatsBump::with_capacity(256);
        let _ = a.alloc_copy(1u8); // 1
        let _ = a.alloc_copy(1u32); // 4
        let _ = a.alloc_copy(1u64); // 8
        assert_eq!(a.stats().bytes_used, 1 + 4 + 8);
    }

    #[test]
    fn bytes_wasted_tracks_padding() {
        let mut a = StatsBump::with_capacity(256);
        // After a 1-byte alloc the cursor is at 1. A u32 forces 3
        // bytes of padding to reach offset 4. A u64 right after lands
        // at offset 8 (no padding because cursor is already at 8).
        let _ = a.alloc_copy(1u8);
        let _ = a.alloc_copy(1u32);
        let _ = a.alloc_copy(1u64);
        assert_eq!(a.stats().bytes_wasted, 3);
    }

    #[test]
    fn peak_persists_across_reset() {
        let mut a = StatsBump::with_capacity(1024);
        for _ in 0..50u64 {
            let _ = a.alloc_copy(0u64);
        }
        let peak_before = a.stats().peak_bytes;
        assert!(peak_before >= 50 * 8);
        a.reset();
        let _ = a.alloc_copy(0u64);
        assert_eq!(a.stats().peak_bytes, peak_before, "peak survives reset");
    }

    #[test]
    fn chunk_count_increments_on_grow() {
        let mut a = StatsBump::with_capacity(64);
        assert_eq!(a.stats().chunk_count, 1);
        for _ in 0..32u64 {
            let _ = a.alloc_copy(0u64);
        }
        assert!(a.stats().chunk_count >= 2, "grow must bump chunk count");
    }

    #[test]
    fn clear_stats_zeroes_counters() {
        let mut a = StatsBump::with_capacity(256);
        for _ in 0..10u64 {
            let _ = a.alloc_copy(0u64);
        }
        assert!(a.stats().allocations > 0);
        a.clear_stats();
        let s = a.stats();
        assert_eq!(s.allocations, 0);
        assert_eq!(s.bytes_used, 0);
        assert_eq!(s.bytes_wasted, 0);
        assert_eq!(s.peak_bytes, 0);
        assert_eq!(s.chunk_count, 1);
    }
}
