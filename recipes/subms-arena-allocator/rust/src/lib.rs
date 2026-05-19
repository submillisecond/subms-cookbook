//! Bump-pointer arena. Allocate fixed-layout values into a growing byte buffer;
//! `reset()` rewinds the bump cursor so the next request can reuse the entire
//! arena.
//!
//! **Important: Drop is NOT run on items in the arena.** Anything with a
//! non-trivial `Drop` (e.g. `String`, `Vec`) will leak its heap allocation
//! if you store it in this arena. See [`Bump::alloc_copy`] which is
//! constrained to `Copy` types as the safe public surface.
//!
//! ```
//! use subms_arena_allocator::Bump;
//! let mut a = Bump::with_capacity(1024);
//! let x: &mut u32 = a.alloc_copy(42u32);
//! assert_eq!(*x, 42);
//! a.reset();
//! ```

use std::alloc::{alloc, dealloc, Layout};
use std::cell::Cell;
use std::mem;
use std::ptr;

/// Bump-pointer arena.
pub struct Bump {
    chunks: Vec<Chunk>,
    /// Current chunk's bump cursor (byte offset into chunks.last()).
    cursor: Cell<usize>,
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

impl Bump {
    /// New empty arena. First chunk is allocated on first use.
    pub fn new() -> Self {
        Self::with_capacity(4096)
    }

    /// New arena, pre-allocating the first chunk at `initial_bytes` bytes.
    pub fn with_capacity(initial_bytes: usize) -> Self {
        let initial_bytes = initial_bytes.max(64);
        let layout = Layout::from_size_align(initial_bytes, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM allocating first arena chunk");
        Self {
            chunks: vec![Chunk { ptr, layout }],
            cursor: Cell::new(0),
        }
    }

    /// Allocate a `Copy` value. Returns an exclusive reference valid until
    /// the next call to [`reset`](Bump::reset) or arena drop.
    pub fn alloc_copy<T: Copy>(&mut self, value: T) -> &mut T {
        let layout = Layout::new::<T>();
        let p = self.alloc_raw(layout);
        unsafe {
            ptr::write(p as *mut T, value);
            &mut *(p as *mut T)
        }
    }

    /// Allocate `size` bytes, aligned to `align`. Returns a raw pointer; the
    /// caller is responsible for writing into it. Used by `alloc_copy`.
    pub fn alloc_raw(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        // align math: round cursor up to alignment boundary
        let cur = self.cursor.get();
        let aligned = align_up(cur, align);
        let end = aligned + size;
        let chunk = self.chunks.last().expect("at least one chunk");
        if end <= chunk.layout.size() {
            self.cursor.set(end);
            return unsafe { chunk.ptr.add(aligned) };
        }
        // Grow: allocate a new chunk twice the previous size, at least `size + align`.
        self.grow(size + align);
        let chunk = self.chunks.last().unwrap();
        let aligned = align_up(0, align);
        self.cursor.set(aligned + size);
        unsafe { chunk.ptr.add(aligned) }
    }

    fn grow(&mut self, min_bytes: usize) {
        let last = self.chunks.last().expect("at least one chunk");
        let new_size = (last.layout.size() * 2).max(min_bytes);
        let layout = Layout::from_size_align(new_size, 16).expect("layout");
        let ptr = unsafe { alloc(layout) };
        assert!(!ptr.is_null(), "OOM growing arena");
        self.chunks.push(Chunk { ptr, layout });
        self.cursor.set(0);
    }

    /// Rewind to empty without freeing chunks. The next `alloc_*` call reuses
    /// the same chunks.
    pub fn reset(&mut self) {
        // Drop all chunks except the largest, then keep that one and reset cursor.
        if self.chunks.len() > 1 {
            let largest = self.chunks
                .iter()
                .enumerate()
                .max_by_key(|(_, c)| c.layout.size())
                .map(|(i, _)| i)
                .unwrap();
            let keeper = self.chunks.swap_remove(largest);
            self.chunks.clear();
            self.chunks.push(keeper);
        }
        self.cursor.set(0);
    }

    /// Total bytes currently allocated across all chunks.
    pub fn total_capacity(&self) -> usize {
        self.chunks.iter().map(|c| c.layout.size()).sum()
    }
}

impl Default for Bump {
    fn default() -> Self {
        Self::new()
    }
}

/// `(p + align - 1) & !(align - 1)` - rounds up to the next aligned address.
fn align_up(p: usize, align: usize) -> usize {
    debug_assert!(align.is_power_of_two(), "alignment must be a power of two");
    (p + align - 1) & !(align - 1)
}

#[cfg(feature = "harness")]
pub mod recipe;

// Silence the unused-mem warning for systems where this gets vendored.
#[allow(dead_code)]
fn _silence_unused() {
    let _ = mem::size_of::<()>();
}
