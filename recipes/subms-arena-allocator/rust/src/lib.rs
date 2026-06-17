//! Fixed-capacity bump-pointer arena. Allocate fixed-layout values into a
//! single pre-sized byte buffer; `reset()` rewinds the bump cursor so the
//! next request can reuse the entire arena.
//!
//! **Drop is NOT run on items in the arena.** Anything with a non-trivial
//! `Drop` (e.g. `String`, `Vec`) will leak its heap allocation if you store
//! it in this arena. [`Bump::alloc_copy`] is constrained to `Copy` types as
//! the safe public surface.
//!
//! Base is single-chunk and fixed-capacity. When the chunk is exhausted
//! `alloc_*` panics and `try_alloc_*` returns `None`. Opt into the
//! `growable` feature for the auto-grow variant.
//!
//! ```
//! use subms_arena_allocator::Bump;
//! let mut a = Bump::with_capacity(1024);
//! let x: &mut u32 = a.alloc_copy(42u32);
//! assert_eq!(*x, 42);
//! a.reset();
//! ```

use std::alloc::{Layout, alloc, dealloc};
use std::ptr;

/// Fixed-capacity bump-pointer arena.
pub struct Bump {
    ptr: *mut u8,
    layout: Layout,
    cursor: usize,
}

impl Bump {
    /// New empty arena with a 4 KiB chunk.
    pub fn new() -> Self {
        Self::with_capacity(4096)
    }

    /// New arena, pre-allocating a single chunk of `capacity` bytes
    /// (promoted to a 64-byte floor and 16-byte alignment).
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(64);
        let layout = Layout::from_size_align(capacity, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating arena chunk");
        Self {
            ptr,
            layout,
            cursor: 0,
        }
    }

    /// Allocate a `Copy` value. Panics if the arena is out of room.
    pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T {
        let cursor = self.cursor;
        let cap = self.layout.size();
        match self.try_alloc_copy(value) {
            Some(r) => r,
            None => panic!(
                "Bump out of capacity: cursor={} layout_size={} requested={}",
                cursor,
                cap,
                std::mem::size_of::<T>(),
            ),
        }
    }

    /// Fallible alloc. Returns `None` if the arena can't fit the value
    /// at its natural alignment.
    pub fn try_alloc_copy<T: Copy>(&mut self, value: T) -> Option<&mut T> {
        let layout = Layout::new::<T>();
        let p = self.try_alloc_raw(layout)?;
        unsafe {
            ptr::write(p as *mut T, value);
            Some(&mut *(p as *mut T))
        }
    }

    /// Allocate `layout.size()` bytes aligned to `layout.align()`.
    /// Panics if the request doesn't fit.
    pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8 {
        let cursor = self.cursor;
        let cap = self.layout.size();
        let requested = layout.size();
        match self.try_alloc_raw(layout) {
            Some(p) => p,
            None => panic!(
                "Bump out of capacity: cursor={cursor} layout_size={cap} requested={requested}",
            ),
        }
    }

    /// Fallible raw alloc. Returns `None` if the request doesn't fit.
    pub fn try_alloc_raw(&mut self, layout: Layout) -> Option<*mut u8> {
        let size = layout.size();
        let align = layout.align();
        let base = self.ptr as usize;
        let aligned_abs = align_up(base + self.cursor, align);
        let aligned = aligned_abs - base;
        let end = aligned.checked_add(size)?;
        if end > self.layout.size() {
            return None;
        }
        self.cursor = end;
        Some(unsafe { self.ptr.add(aligned) })
    }

    /// Rewind to empty. The buffer is retained for reuse.
    pub fn reset(&mut self) {
        self.cursor = 0;
    }

    /// Bytes used so far in the current chunk.
    pub fn used(&self) -> usize {
        self.cursor
    }

    /// Total capacity of the single backing chunk.
    pub fn capacity(&self) -> usize {
        self.layout.size()
    }

    /// Backwards-compatible alias for [`capacity`].
    pub fn total_capacity(&self) -> usize {
        self.capacity()
    }
}

impl Default for Bump {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for Bump {
    fn drop(&mut self) {
        unsafe { dealloc(self.ptr, self.layout) };
    }
}

/// `(p + align - 1) & !(align - 1)` - rounds up to the next aligned address.
#[inline]
pub(crate) fn align_up(p: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "alignment must be a power of two");
    (p + align - 1) & !(align - 1)
}

#[cfg(feature = "harness")]
pub mod recipe;

#[cfg(any(
    feature = "typed",
    feature = "growable",
    feature = "stats",
    feature = "aligned",
    feature = "freelist",
))]
pub mod features;

#[cfg(feature = "aligned")]
pub use features::aligned::AlignedBump;
#[cfg(feature = "freelist")]
pub use features::freelist::FreelistBump;
#[cfg(feature = "growable")]
pub use features::growable::GrowableBump;
#[cfg(feature = "stats")]
pub use features::stats::{BumpStats, StatsBump};
#[cfg(feature = "typed")]
pub use features::typed::TypedArena;
