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
#[should_panic(expected = "TypedArena full")]
fn alloc_panics_when_full() {
    let a = TypedArena::<u32>::with_capacity(2);
    a.alloc(1);
    a.alloc(2);
    a.alloc(3); // over capacity -> panics
}

#[test]
fn capacity_and_is_empty_report_state() {
    let a = TypedArena::<u32>::with_capacity(8);
    assert_eq!(a.capacity(), 8);
    assert!(a.is_empty());
    a.alloc(1);
    assert!(!a.is_empty());
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
