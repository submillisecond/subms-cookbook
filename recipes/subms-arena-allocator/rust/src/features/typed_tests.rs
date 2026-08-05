use super::*;

#[test]
fn alloc_returns_handle_that_reads_back() {
    let mut a = TypedArena::<u32>::with_capacity(16);
    let s = a.alloc(42);
    assert_eq!(*a.get(&s), 42);
    assert_eq!(a.len(), 1);
}

#[test]
fn get_mut_mutates_in_place() {
    let mut a = TypedArena::<u32>::with_capacity(16);
    let s = a.alloc(42);
    *a.get_mut(&s) = 99;
    assert_eq!(*a.get(&s), 99);
}

#[test]
fn slot_index_follows_allocation_order() {
    let mut a = TypedArena::<u32>::with_capacity(4);
    let s0 = a.alloc(10);
    let s1 = a.alloc(20);
    assert_eq!(s0.index(), 0);
    assert_eq!(s1.index(), 1);
}

#[test]
fn fills_to_capacity_then_try_alloc_hands_the_value_back() {
    let mut a = TypedArena::<u64>::with_capacity(4);
    for i in 0..4u64 {
        a.alloc(i);
    }
    assert_eq!(a.try_alloc(99), Err(99), "full arena returns the value");
    assert_eq!(a.len(), 4);
}

#[test]
#[should_panic(expected = "TypedArena full")]
fn alloc_panics_when_full() {
    let mut a = TypedArena::<u32>::with_capacity(2);
    a.alloc(1);
    a.alloc(2);
    a.alloc(3);
}

#[test]
fn freed_slot_is_reused() {
    let mut a = TypedArena::<u32>::with_capacity(8);
    let s = a.alloc(1);
    let idx = s.index();
    a.free(s);
    assert_eq!(a.len(), 0, "freeing drops the live count");
    let reused = a.alloc(2);
    assert_eq!(reused.index(), idx, "freed slot comes back");
    assert_eq!(*a.get(&reused), 2);
    assert_eq!(a.reuse_hits(), 1);
}

#[test]
fn reuse_is_lifo() {
    let mut a = TypedArena::<u32>::with_capacity(8);
    let s0 = a.alloc(0);
    let s1 = a.alloc(1);
    let s2 = a.alloc(2);
    a.free(s0);
    a.free(s1);
    a.free(s2);
    assert_eq!(a.alloc(10).index(), 2, "last freed is first reused");
    assert_eq!(a.alloc(11).index(), 1);
    assert_eq!(a.alloc(12).index(), 0);
    assert_eq!(a.reuse_hits(), 3);
}

#[test]
fn reuse_hits_counts_only_recycled_allocs() {
    let mut a = TypedArena::<u32>::with_capacity(8);
    let s = a.alloc(1);
    a.alloc(2);
    assert_eq!(a.reuse_hits(), 0, "fresh slots are not reuse");
    a.free(s);
    a.alloc(3);
    assert_eq!(a.reuse_hits(), 1);
}

#[test]
fn reuse_keeps_the_high_water_mark_flat() {
    // The reason the free list exists: churn inside one arena lifetime
    // must not consume fresh slots.
    let mut a = TypedArena::<u64>::with_capacity(2);
    for i in 0..1_000u64 {
        let s = a.alloc(i);
        assert_eq!(*a.get(&s), i);
        a.free(s);
    }
    assert_eq!(a.reuse_hits(), 999);
    assert!(a.is_empty());
    a.alloc(0);
    a.alloc(1);
    assert_eq!(a.len(), 2, "both slots still available after the churn");
}

#[test]
fn reused_slot_overwrites_the_previous_occupant() {
    // A freed slot keeps its old value until something reallocates it.
    // Java can read that stale value back; here `free` consumes the
    // handle, so the only observable path is the reuse, which writes.
    let mut a = TypedArena::<u32>::with_capacity(2);
    let s = a.alloc(7);
    a.free(s);
    let reused = a.alloc(9);
    assert_eq!(*a.get(&reused), 9);
}

#[test]
fn reset_clears_slots_free_list_and_counter() {
    let mut a = TypedArena::<u32>::with_capacity(4);
    let s = a.alloc(1);
    a.alloc(2);
    a.free(s);
    a.alloc(3);
    assert_eq!(a.reuse_hits(), 1);
    a.reset();
    assert_eq!(a.len(), 0);
    assert!(a.is_empty());
    assert_eq!(a.reuse_hits(), 0);
    assert_eq!(a.capacity(), 4, "capacity survives reset");
}

#[test]
fn reset_then_alloc_starts_from_slot_zero() {
    let mut a = TypedArena::<u32>::with_capacity(4);
    a.alloc(1);
    a.alloc(2);
    a.reset();
    assert_eq!(a.alloc(3).index(), 0);
}

#[test]
fn capacity_and_is_empty_report_state() {
    let mut a = TypedArena::<u32>::with_capacity(8);
    assert_eq!(a.capacity(), 8);
    assert!(a.is_empty());
    let s = a.alloc(1);
    assert!(!a.is_empty());
    a.free(s);
    assert!(a.is_empty());
}

#[test]
fn capacity_is_floored_at_one() {
    let mut a = TypedArena::<u32>::with_capacity(0);
    assert_eq!(a.capacity(), 1);
    let s = a.alloc(1);
    assert_eq!(a.try_alloc(2), Err(2));
    a.free(s);
    assert!(a.try_alloc(2).is_ok(), "the single slot is reusable");
}

#[test]
fn holds_a_cache_line_sized_type() {
    #[derive(Copy, Clone, Debug, PartialEq)]
    #[repr(align(64))]
    struct CacheLine([u8; 64]);
    let mut a = TypedArena::<CacheLine>::with_capacity(4);
    let s = a.alloc(CacheLine([7u8; 64]));
    assert_eq!(*a.get(&s), CacheLine([7u8; 64]));
    assert_eq!(a.len(), 1);
}

#[test]
fn slot_is_debuggable_and_comparable() {
    let mut a = TypedArena::<u32>::with_capacity(2);
    let s0 = a.alloc(1);
    let s1 = a.alloc(2);
    assert_ne!(s0, s1);
    assert_eq!(format!("{s0:?}"), "Slot(0)");
}
