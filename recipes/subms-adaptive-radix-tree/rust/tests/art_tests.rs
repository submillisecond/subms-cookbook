use subms_adaptive_radix_tree::Art;

#[test]
fn insert_and_get() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"alice", 1);
    t.insert(b"bob", 2);
    t.insert(b"alex", 3);
    assert_eq!(t.get(b"alice").copied(), Some(1));
    assert_eq!(t.get(b"bob").copied(), Some(2));
    assert_eq!(t.get(b"alex").copied(), Some(3));
    assert_eq!(t.get(b"missing"), None);
    assert_eq!(t.len(), 3);
}

#[test]
fn insert_replaces_value() {
    let mut t: Art<&'static str> = Art::new();
    assert!(t.insert(b"k", "first").is_none());
    assert_eq!(t.insert(b"k", "second"), Some("first"));
    assert_eq!(t.get(b"k").copied(), Some("second"));
    assert_eq!(t.len(), 1);
}

#[test]
fn empty_key_supported() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"", 42);
    assert_eq!(t.get(b"").copied(), Some(42));
    assert_eq!(t.len(), 1);
}

#[test]
fn many_keys_force_node_growth() {
    let mut t: Art<i32> = Art::new();
    // 256 distinct first-byte keys at the root forces the root to grow
    // from Small (4 slots) to Full (256-way).
    for i in 0..=255u8 {
        let key = [i, 0u8, 0u8];
        t.insert(&key, i as i32);
    }
    assert_eq!(t.len(), 256);
    for i in 0..=255u8 {
        let key = [i, 0u8, 0u8];
        assert_eq!(t.get(&key).copied(), Some(i as i32), "key starting {i}");
    }
}

#[test]
fn shared_prefixes() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"prefix/a", 1);
    t.insert(b"prefix/b", 2);
    t.insert(b"prefix/c", 3);
    assert_eq!(t.get(b"prefix/a").copied(), Some(1));
    assert_eq!(t.get(b"prefix/b").copied(), Some(2));
    assert_eq!(t.get(b"prefix/c").copied(), Some(3));
    assert_eq!(t.get(b"prefix").copied(), None);
}

#[test]
fn empty_tree_is_empty() {
    let t: Art<i32> = Art::new();
    assert!(t.is_empty());
    assert_eq!(t.len(), 0);
    assert!(t.get(b"any").is_none());
}

#[test]
fn long_keys() {
    let mut t: Art<u32> = Art::new();
    let key = vec![b'a'; 1000];
    t.insert(&key, 42);
    assert_eq!(t.get(&key).copied(), Some(42));
}

#[test]
fn binary_keys_with_zero_bytes() {
    let mut t: Art<u32> = Art::new();
    t.insert(&[0u8, 1, 2], 1);
    t.insert(&[0u8, 1, 3], 2);
    t.insert(&[0u8, 2, 0], 3);
    assert_eq!(t.get(&[0u8, 1, 2]).copied(), Some(1));
    assert_eq!(t.get(&[0u8, 1, 3]).copied(), Some(2));
    assert_eq!(t.get(&[0u8, 2, 0]).copied(), Some(3));
}

#[test]
fn shorter_key_is_distinct_from_longer_with_same_prefix() {
    let mut t: Art<i32> = Art::new();
    t.insert(b"foo", 1);
    t.insert(b"foobar", 2);
    assert_eq!(t.get(b"foo").copied(), Some(1));
    assert_eq!(t.get(b"foobar").copied(), Some(2));
    assert!(t.get(b"foob").is_none());
    assert!(t.get(b"foobaz").is_none());
}

#[test]
fn default_constructor() {
    let t: Art<i32> = Art::default();
    assert!(t.is_empty());
}
