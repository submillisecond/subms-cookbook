//! `GrowableBump`: a bump arena that allocates a fresh chunk (at twice
//! the previous size, or the request size, whichever is larger) when
//! the active chunk runs out of room. Previous chunks are retained
//! until `reset()` or drop so any references handed out remain valid.
//!
//! Trade-off vs the fixed-capacity base: grow events are not free -
//! they cost an allocator round-trip and break the steady-state p99.
//! `reset()` keeps only the largest chunk, so steady-state workloads
//! converge on a single chunk after the first round.

use std::alloc::{Layout, alloc, dealloc};
use std::ptr;

use crate::align_up;

/// Multi-chunk bump-pointer arena that auto-grows on exhaustion.
pub struct GrowableBump {
    chunks: Vec<Chunk>,
    /// Cursor into `chunks.last()`.
    cursor: usize,
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

impl GrowableBump {
    /// New arena with a 4 KiB initial chunk.
    pub fn new() -> Self {
        Self::with_capacity(4096)
    }

    /// New arena with the requested initial chunk size (64-byte floor,
    /// 16-byte aligned chunk allocation).
    pub fn with_capacity(initial: usize) -> Self {
        let initial = initial.max(64);
        let layout = Layout::from_size_align(initial, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating first growable chunk");
        Self {
            chunks: vec![Chunk { ptr, layout }],
            cursor: 0,
        }
    }

    /// Allocate a `Copy` value. Grows on exhaustion.
    pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T {
        let layout = Layout::new::<T>();
        let p = self.alloc_raw(layout);
        unsafe {
            ptr::write(p as *mut T, value);
            &mut *(p as *mut T)
        }
    }

    /// Allocate `layout.size()` bytes aligned to `layout.align()`.
    /// Grows on exhaustion.
    pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        let last_ptr;
        let last_size;
        {
            let last = self.chunks.last().expect("at least one chunk");
            last_ptr = last.ptr;
            last_size = last.layout.size();
        }
        let base = last_ptr as usize;
        let aligned = align_up(base + self.cursor, align) - base;
        let end = aligned + size;
        if end <= last_size {
            self.cursor = end;
            return unsafe { last_ptr.add(aligned) };
        }
        // Grow: 2x the previous chunk size, but at least `size + align`
        // so the new chunk can definitely serve the pending request.
        self.grow(size + align);
        let last_ptr = self.chunks.last().unwrap().ptr;
        let base = last_ptr as usize;
        let aligned = align_up(base, align) - base;
        self.cursor = aligned + size;
        unsafe { last_ptr.add(aligned) }
    }

    fn grow(&mut self, min_bytes: usize) {
        let last = self.chunks.last().expect("at least one chunk");
        let new_size = (last.layout.size() * 2).max(min_bytes);
        let layout = Layout::from_size_align(new_size, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM growing arena");
        self.chunks.push(Chunk { ptr, layout });
        self.cursor = 0;
    }

    /// Rewind every chunk. Keeps only the largest chunk; smaller
    /// chunks are dropped. Subsequent allocations reuse the kept chunk
    /// without further grow events for workloads within its size.
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

    /// Bytes allocated across every retained chunk.
    pub fn total_capacity(&self) -> usize {
        self.chunks.iter().map(|c| c.layout.size()).sum()
    }

    /// Number of chunks currently retained.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }
}

impl Default for GrowableBump {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
#[path = "growable_tests.rs"]
mod tests;
