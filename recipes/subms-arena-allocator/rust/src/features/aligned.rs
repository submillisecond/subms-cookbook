//! `AlignedBump`: bump arena exposing an `alloc_aligned(size, align)`
//! convenience over the base `Layout`-driven raw API.
//!
//! The base `Bump::alloc_raw` already accepts an arbitrary `Layout`
//! and respects its alignment. This module exists as a focused
//! shape for callers who think in (bytes, alignment) pairs:
//! cache-line aligned scratch buffers for SIMD, page-aligned scratch
//! for DMA, etc.
//!
//! Fixed-capacity, single chunk. Panics on OOM; use `try_alloc_aligned`
//! for the fallible form.

use std::alloc::{Layout, alloc, dealloc};
use std::slice;

use crate::align_up;

/// Bump arena exposing explicit per-allocation alignment.
pub struct AlignedBump {
    ptr: *mut u8,
    layout: Layout,
    cursor: usize,
}

impl AlignedBump {
    /// New arena with the given capacity. The backing buffer itself is
    /// allocated 64-byte aligned so cache-line requests within
    /// `capacity` always succeed.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(64);
        // 64-byte chunk alignment so the first cache-line request
        // costs zero padding.
        let layout = Layout::from_size_align(capacity, 64).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating aligned arena chunk");
        Self {
            ptr,
            layout,
            cursor: 0,
        }
    }

    /// Allocate `size` bytes aligned to `align` (must be a power of two).
    /// Panics if the request doesn't fit.
    pub fn alloc_aligned(&mut self, size: usize, align: usize) -> &mut [u8] {
        let cursor = self.cursor;
        let cap = self.layout.size();
        match self.try_alloc_aligned(size, align) {
            Some(s) => s,
            None => panic!(
                "AlignedBump out of capacity: cursor={cursor} cap={cap} size={size} align={align}",
            ),
        }
    }

    /// Fallible aligned alloc. Returns `None` if the request doesn't fit.
    pub fn try_alloc_aligned(&mut self, size: usize, align: usize) -> Option<&mut [u8]> {
        assert!(
            align.is_power_of_two(),
            "align must be power of two: {align}"
        );
        let base = self.ptr as usize;
        let aligned = align_up(base + self.cursor, align) - base;
        let end = aligned.checked_add(size)?;
        if end > self.layout.size() {
            return None;
        }
        self.cursor = end;
        unsafe {
            let p = self.ptr.add(aligned);
            Some(slice::from_raw_parts_mut(p, size))
        }
    }

    /// Rewind. Buffer retained for reuse.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Total capacity.
    pub fn capacity(&self) -> usize {
        self.layout.size()
    }

    /// Bytes used so far.
    pub fn used(&self) -> usize {
        self.cursor
    }
}

impl Drop for AlignedBump {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

#[cfg(test)]
#[path = "aligned_tests.rs"]
mod tests;
