//! `FreelistBump`: bump arena with a per-size freelist for reuse.
//!
//! Calling `free(ptr, layout)` returns the freed region to a bucket
//! keyed by `(size, align)`. Subsequent `alloc` calls with a matching
//! layout pop from the bucket before consulting the bump cursor.
//!
//! Trade-off vs the plain base: alloc gains a bucket lookup (an
//! `entry` on a small `HashMap`); the win is real for size-uniform
//! workloads (object pools, fixed-shape graph nodes) because reused
//! slots come back warm in cache and the bump cursor stops advancing.
//!
//! Reused slots are returned at the same `(size, align)`. We do not
//! split buckets across sizes - reusing a 64-byte slot to satisfy an
//! 8-byte request would lose 56 bytes per call.
//!
//! Caller MUST pass the same `Layout` to `free()` that was used to
//! `alloc()` the pointer. Mismatched frees are detected only as a
//! debug-assertion; release builds will silently mis-bucket.

use std::alloc::{Layout, alloc, dealloc};
use std::collections::HashMap;
use std::ptr;

use crate::align_up;

/// Bump arena with a per-`(size, align)` freelist.
pub struct FreelistBump {
    ptr: *mut u8,
    layout: Layout,
    cursor: usize,
    /// Keyed on `(size, align)`. Value is a stack of recyclable
    /// pointers into the buffer.
    buckets: HashMap<(usize, usize), Vec<*mut u8>>,
    /// Allocations served from the buckets (free reuse). Useful for
    /// validating the freelist actually fires.
    reuse_hits: u64,
}

impl FreelistBump {
    pub fn new() -> Self {
        Self::with_capacity(4096)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(64);
        let layout = Layout::from_size_align(capacity, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating freelist arena chunk");
        Self {
            ptr,
            layout,
            cursor: 0,
            buckets: HashMap::new(),
            reuse_hits: 0,
        }
    }

    /// Allocate a `Copy` value. Checks the freelist first.
    pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T {
        let layout = Layout::new::<T>();
        let p = self.alloc_raw(layout);
        unsafe {
            ptr::write(p as *mut T, value);
            &mut *(p as *mut T)
        }
    }

    /// Allocate via the freelist if a matching bucket has entries,
    /// else bump-allocate. Panics on OOM.
    pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8 {
        let key = (layout.size(), layout.align());
        if let Some(stack) = self.buckets.get_mut(&key) {
            if let Some(p) = stack.pop() {
                self.reuse_hits += 1;
                return p;
            }
        }
        self.bump_alloc_raw(layout)
            .expect("FreelistBump out of capacity")
    }

    fn bump_alloc_raw(&mut self, layout: Layout) -> Option<*mut u8> {
        let size = layout.size();
        let align = layout.align();
        let base = self.ptr as usize;
        let aligned = align_up(base + self.cursor, align) - base;
        let end = aligned.checked_add(size)?;
        if end > self.layout.size() {
            return None;
        }
        self.cursor = end;
        Some(unsafe { self.ptr.add(aligned) })
    }

    /// Return `ptr` to the freelist. The caller MUST pass the layout
    /// the original `alloc` used.
    ///
    /// # Safety
    /// `ptr` must have come from this arena's `alloc*` and not been
    /// freed since. The arena does not validate this in release builds.
    pub unsafe fn free(&mut self, ptr_to_free: *mut u8, layout: Layout) {
        debug_assert!(
            self.contains(ptr_to_free),
            "FreelistBump::free given pointer outside the arena",
        );
        let key = (layout.size(), layout.align());
        self.buckets.entry(key).or_default().push(ptr_to_free);
    }

    fn contains(&self, p: *mut u8) -> bool {
        let base = self.ptr as usize;
        let end = base + self.layout.size();
        let q = p as usize;
        q >= base && q < end
    }

    /// Drop the freelist and rewind. Subsequent allocs all come from
    /// the bump path until items get freed again.
    pub fn reset(&mut self) {
        self.buckets.clear();
        self.cursor = 0;
        self.reuse_hits = 0;
    }

    /// Bytes used by bump-allocation so far (excludes freelist slots).
    pub fn used(&self) -> usize {
        self.cursor
    }

    /// Total capacity of the backing chunk.
    pub fn capacity(&self) -> usize {
        self.layout.size()
    }

    /// How many allocs have been served from the freelist since
    /// construction or the last `reset()`.
    pub fn reuse_hits(&self) -> u64 {
        self.reuse_hits
    }

    /// Sum of slots currently held across every bucket.
    pub fn freelist_len(&self) -> usize {
        self.buckets.values().map(|v| v.len()).sum()
    }
}

impl Default for FreelistBump {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for FreelistBump {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

#[cfg(test)]
#[path = "freelist_tests.rs"]
mod tests;
