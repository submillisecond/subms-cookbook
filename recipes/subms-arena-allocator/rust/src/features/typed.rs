//! `TypedArena<T>`: a bump arena parametric over a single `Copy` type.
//!
//! When the caller knows the element type at compile time, the arena
//! can offer a tighter surface: every `alloc()` returns `&mut T` with no
//! `Layout` dance, no alignment padding between elements of the same
//! type, and no possibility of mixing element shapes inside the arena.
//!
//! Implemented over a `Vec<T>` rather than raw bytes - this avoids
//! unsafe pointer arithmetic for the common case and lets the borrow
//! checker keep the references honest. The Vec is preallocated to the
//! requested capacity and never reallocated (preserving stable
//! references between `alloc` calls).
//!
//! Restricted to `Copy` because the arena does not run destructors on
//! `reset()` or drop. Holding `String` here would leak the heap buffer.

use std::cell::UnsafeCell;
use std::marker::PhantomData;

/// Bump arena specialised to a single element type.
pub struct TypedArena<T: Copy> {
    /// `UnsafeCell` so we can hand out `&mut T` while the arena is
    /// borrowed `&` (the existing references don't alias because each
    /// `alloc()` bumps the cursor before yielding the new slot).
    slots: UnsafeCell<Vec<T>>,
    capacity: usize,
    _marker: PhantomData<T>,
}

impl<T: Copy> TypedArena<T> {
    /// New arena with room for `capacity` elements. The backing
    /// `Vec<T>` is preallocated; subsequent `alloc()` calls don't
    /// reallocate.
    pub fn with_capacity(capacity: usize) -> Self {
        let cap = capacity.max(1);
        Self {
            slots: UnsafeCell::new(Vec::with_capacity(cap)),
            capacity: cap,
            _marker: PhantomData,
        }
    }

    /// Allocate `value`. Panics if the arena is full.
    ///
    /// Takes `&self` so the arena can hand out non-aliasing `&mut T`
    /// references across multiple `alloc()` calls. Interior mutability
    /// via `UnsafeCell`; safety relies on never reallocating the
    /// backing `Vec` and never re-issuing a slot.
    #[allow(clippy::mut_from_ref)]
    pub fn alloc(&self, value: T) -> &mut T {
        match self.try_alloc(value) {
            Some(r) => r,
            None => panic!(
                "TypedArena full: capacity={} len={}",
                self.capacity,
                self.len(),
            ),
        }
    }

    /// Fallible allocate. Returns `None` if the arena is full.
    #[allow(clippy::mut_from_ref)]
    pub fn try_alloc(&self, value: T) -> Option<&mut T> {
        // SAFETY: we never resize the Vec past `capacity`, so existing
        // `&mut T` references into the arena keep pointing at stable
        // memory. We never hand out two references to the same slot:
        // each `alloc()` pushes a new element and returns the new
        // slot's address.
        let slots = unsafe { &mut *self.slots.get() };
        if slots.len() >= self.capacity {
            return None;
        }
        slots.push(value);
        let idx = slots.len() - 1;
        Some(unsafe { &mut *slots.as_mut_ptr().add(idx) })
    }

    /// Number of items currently held.
    pub fn len(&self) -> usize {
        unsafe { (*self.slots.get()).len() }
    }

    /// True when no items have been allocated.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total slots available.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Forget every previously-allocated item. References returned
    /// from `alloc()` are invalidated; the borrow checker enforces
    /// this because `reset()` takes `&mut self`.
    pub fn reset(&mut self) {
        self.slots.get_mut().clear();
    }

    /// Iterate the live items as immutable references.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        // SAFETY: an immutable borrow of the arena blocks any
        // concurrent `alloc()` because `alloc()` is `&self` and the
        // returned `&mut T` would alias.
        unsafe { (*self.slots.get()).iter() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn alloc_returns_typed_ref() {
        let a = TypedArena::<u32>::with_capacity(16);
        let r = a.alloc(42);
        assert_eq!(*r, 42);
        *r = 99;
        assert_eq!(*r, 99);
    }

    #[test]
    fn fills_to_capacity_then_rejects() {
        let a = TypedArena::<u64>::with_capacity(4);
        for i in 0..4 {
            a.alloc(i as u64);
        }
        assert!(a.try_alloc(99).is_none(), "full arena must refuse");
        assert_eq!(a.len(), 4);
    }

    #[test]
    fn reset_invalidates_and_reuses() {
        let mut a = TypedArena::<u32>::with_capacity(2);
        a.alloc(1);
        a.alloc(2);
        assert_eq!(a.len(), 2);
        a.reset();
        assert_eq!(a.len(), 0);
        a.alloc(3);
        a.alloc(4);
        assert_eq!(a.len(), 2);
    }

    #[test]
    fn alignment_one_byte_type() {
        // Byte allocations: every slot is consecutive, no padding.
        let a = TypedArena::<u8>::with_capacity(32);
        let p1 = a.alloc(1) as *mut u8 as usize;
        let p2 = a.alloc(2) as *mut u8 as usize;
        assert_eq!(p2 - p1, 1, "u8 slots must be contiguous");
    }

    #[test]
    fn alignment_64_byte_type() {
        // A cache-line-sized type stays naturally aligned per slot.
        #[derive(Copy, Clone)]
        #[repr(align(64))]
        #[allow(dead_code)]
        struct CacheLine([u8; 64]);
        let a = TypedArena::<CacheLine>::with_capacity(4);
        for _ in 0..4 {
            let p = a.alloc(CacheLine([0u8; 64])) as *mut CacheLine as usize;
            assert_eq!(p % 64, 0, "CacheLine must be 64-byte aligned");
        }
    }

    #[test]
    fn references_remain_stable_across_alloc() {
        // The whole reason for using TypedArena over Vec<T> is stable
        // references. Preallocated Vec must not reallocate as we fill.
        let a = TypedArena::<u32>::with_capacity(8);
        let r0 = a.alloc(10);
        let p0 = r0 as *const u32 as usize;
        for i in 1..8 {
            a.alloc(i);
        }
        // r0's address must still be the same slot.
        assert_eq!(*r0, 10);
        assert_eq!(r0 as *const u32 as usize, p0);
    }

    #[test]
    fn iter_yields_all_allocated() {
        let a = TypedArena::<u32>::with_capacity(8);
        for i in 0..5u32 {
            a.alloc(i);
        }
        let collected: Vec<u32> = a.iter().copied().collect();
        assert_eq!(collected, vec![0, 1, 2, 3, 4]);
    }
}
