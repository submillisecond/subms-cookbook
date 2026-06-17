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
mod tests {
    use super::*;

    #[test]
    fn alignment_one_packs_tightly() {
        let mut a = AlignedBump::with_capacity(256);
        {
            let s1 = a.alloc_aligned(3, 1);
            assert_eq!(s1.len(), 3);
        }
        {
            let s2 = a.alloc_aligned(3, 1);
            assert_eq!(s2.len(), 3);
        }
        // Two byte-aligned 3-byte regions take 6 bytes total.
        assert_eq!(a.used(), 6);
    }

    #[test]
    fn alignment_64_byte_cache_line() {
        let mut a = AlignedBump::with_capacity(512);
        // Burn one byte so the next alloc has to pad.
        let _ = a.alloc_aligned(1, 1);
        let s = a.alloc_aligned(64, 64);
        let p = s.as_ptr() as usize;
        assert_eq!(p % 64, 0, "cache-line alignment");
    }

    #[test]
    fn alignment_512_byte_page_ish() {
        // Higher-than-base alignment: exercise the padding math.
        let mut a = AlignedBump::with_capacity(4096);
        let _ = a.alloc_aligned(7, 1);
        let s = a.alloc_aligned(128, 512);
        let p = s.as_ptr() as usize;
        assert_eq!(p % 512, 0);
    }

    #[test]
    fn rejects_non_power_of_two_align() {
        let mut a = AlignedBump::with_capacity(64);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            // Drop the returned slice immediately so it doesn't escape.
            let _ = a.alloc_aligned(8, 3);
        }));
        assert!(r.is_err());
    }

    #[test]
    fn out_of_capacity_returns_none() {
        let mut a = AlignedBump::with_capacity(64);
        // Capacity is at least 64; 64 + 64 will overflow.
        let _ = a.alloc_aligned(64, 1);
        assert!(a.try_alloc_aligned(64, 64).is_none());
    }

    #[test]
    fn reset_rewinds_cursor() {
        let mut a = AlignedBump::with_capacity(128);
        let _ = a.alloc_aligned(64, 64);
        assert_eq!(a.used(), 64);
        a.reset();
        assert_eq!(a.used(), 0);
        let _ = a.alloc_aligned(64, 64);
        assert_eq!(a.used(), 64);
    }

    #[test]
    fn slice_is_writable() {
        let mut a = AlignedBump::with_capacity(256);
        let s = a.alloc_aligned(16, 8);
        for (i, b) in s.iter_mut().enumerate() {
            *b = i as u8;
        }
        // Re-borrow via fresh alloc would alias, so check ptr equality
        // by re-walking the cursor difference. Simpler: just confirm
        // the writes stuck via the same slice ref.
        for (i, b) in s.iter().enumerate() {
            assert_eq!(*b, i as u8);
        }
    }
}
