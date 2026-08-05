//! `TypedArena<T>`: a typed arena of fixed slots, with slot reuse.
//!
//! Allocation returns an opaque [`Slot`] handle instead of a reference.
//! That is the one shape both ports implement identically: Java cannot
//! put objects inside arena memory without leaving safe Java, but an
//! index into a preallocated slot array is the same idea in both, and
//! the property the sub-ms claim rests on - no allocator call on the hot
//! path - holds either way.
//!
//! Handles also make the Rust side safe. Storage is a `Vec<T>` sized at
//! construction and never reallocated; the free list is preallocated to
//! the same capacity so `free` never allocates either. No `UnsafeCell`,
//! no `unsafe`.
//!
//! `free` takes the handle by value, so the obvious use-after-free does
//! not compile. What is still possible is reading a slot whose index was
//! recycled: the storage keeps whatever the previous occupant left until
//! something allocates over it, and a caller holding a stale index reads
//! that. It is a caller bug, not undefined behaviour - the arena only
//! ever reads storage it owns.
//!
//! Restricted to `Copy` because the arena runs no destructors on `free`,
//! `reset` or drop. Holding a `String` here would leak the heap buffer.

/// Opaque handle to one slot. Carries no arena identity: handing a slot
/// from one arena to another reads that arena's storage at the same
/// index.
#[derive(Debug, PartialEq, Eq, Hash)]
pub struct Slot(usize);

impl Slot {
    /// Position of the slot in the arena's storage.
    pub fn index(&self) -> usize {
        self.0
    }
}

/// Fixed-capacity arena of `T` slots with a LIFO free list.
pub struct TypedArena<T: Copy> {
    slots: Vec<T>,
    free: Vec<usize>,
    capacity: usize,
    reuse_hits: u64,
}

impl<T: Copy> TypedArena<T> {
    /// New arena with room for `capacity` slots, floored at 1. Both the
    /// storage and the free list are reserved up front, so no `alloc`
    /// or `free` reallocates and no handle is invalidated by growth.
    pub fn with_capacity(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            slots: Vec::with_capacity(capacity),
            free: Vec::with_capacity(capacity),
            capacity,
            reuse_hits: 0,
        }
    }

    /// Allocate `value` into a freed slot if one is available, else into
    /// a fresh slot. Panics when the arena is full.
    pub fn alloc(&mut self, value: T) -> Slot {
        match self.try_alloc(value) {
            Ok(slot) => slot,
            Err(_) => panic!(
                "TypedArena full: capacity={} len={}",
                self.capacity,
                self.len()
            ),
        }
    }

    /// Fallible allocate. Hands `value` back when the arena is full, so
    /// the caller does not lose it to a failed insert.
    pub fn try_alloc(&mut self, value: T) -> Result<Slot, T> {
        if let Some(idx) = self.free.pop() {
            self.slots[idx] = value;
            self.reuse_hits += 1;
            return Ok(Slot(idx));
        }
        if self.slots.len() >= self.capacity {
            return Err(value);
        }
        self.slots.push(value);
        Ok(Slot(self.slots.len() - 1))
    }

    /// Read the value in `slot`.
    pub fn get(&self, slot: &Slot) -> &T {
        &self.slots[slot.0]
    }

    /// Mutate the value in `slot` in place.
    pub fn get_mut(&mut self, slot: &Slot) -> &mut T {
        &mut self.slots[slot.0]
    }

    /// Return `slot` to the free list. Takes the handle by value: the
    /// caller cannot read the slot through it afterwards.
    pub fn free(&mut self, slot: Slot) {
        self.free.push(slot.0);
    }

    /// Forget every slot, live or freed, and zero `reuse_hits`. Storage
    /// is retained; every handle issued before the reset is stale.
    pub fn reset(&mut self) {
        self.slots.clear();
        self.free.clear();
        self.reuse_hits = 0;
    }

    /// Slots currently allocated, excluding freed ones.
    pub fn len(&self) -> usize {
        self.slots.len() - self.free.len()
    }

    /// True when no slot is currently allocated.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Total slots available.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Allocations served from the free list since construction or the
    /// last `reset()`. Confirms reuse is actually firing.
    pub fn reuse_hits(&self) -> u64 {
        self.reuse_hits
    }
}

#[cfg(test)]
#[path = "typed_tests.rs"]
mod tests;
