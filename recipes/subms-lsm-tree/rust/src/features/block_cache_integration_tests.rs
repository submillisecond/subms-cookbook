use super::*;

fn block(bytes: &[u8]) -> Block {
    Arc::from(bytes.to_vec().into_boxed_slice())
}

#[test]
fn miss_then_hit() {
    let c = LruBlockCache::new(4);
    let k = BlockKey::new(1, 0);
    assert!(c.get(&k).is_none());
    c.put(k, block(b"payload"));
    let got = c.get(&k).unwrap();
    assert_eq!(&*got, b"payload");
    assert_eq!(c.hits(), 1);
    assert_eq!(c.misses(), 1);
}

#[test]
fn lru_evicts_coldest() {
    let c = LruBlockCache::new(2);
    let a = BlockKey::new(1, 0);
    let b = BlockKey::new(2, 0);
    let z = BlockKey::new(3, 0);
    c.put(a, block(b"A"));
    c.put(b, block(b"B"));
    // Touch a so b is now the LRU.
    c.get(&a);
    c.put(z, block(b"Z"));
    assert!(c.get(&a).is_some(), "recently-used 'a' stays");
    assert!(c.get(&b).is_none(), "stale 'b' evicted");
    assert!(c.get(&z).is_some(), "newest 'z' kept");
}

#[test]
fn put_of_existing_key_refreshes_value() {
    let c = LruBlockCache::new(2);
    let k = BlockKey::new(1, 0);
    c.put(k, block(b"old"));
    c.put(k, block(b"new"));
    assert_eq!(&*c.get(&k).unwrap(), b"new");
    assert_eq!(c.len(), 1);
}

#[test]
fn clear_drops_everything() {
    let c = LruBlockCache::new(4);
    c.put(BlockKey::new(1, 0), block(b"x"));
    c.put(BlockKey::new(2, 0), block(b"y"));
    assert_eq!(c.len(), 2);
    c.clear();
    assert_eq!(c.len(), 0);
    assert!(c.is_empty());
}

#[test]
fn capacity_floor_is_one() {
    let c = LruBlockCache::new(0);
    assert_eq!(c.capacity(), 1);
    c.put(BlockKey::new(1, 0), block(b"a"));
    c.put(BlockKey::new(2, 0), block(b"b"));
    assert_eq!(c.len(), 1, "cap-1 cache holds the latest only");
    assert!(c.get(&BlockKey::new(2, 0)).is_some());
    assert!(c.get(&BlockKey::new(1, 0)).is_none());
}

#[test]
fn block_arc_clone_is_shared() {
    let c = LruBlockCache::new(2);
    let k = BlockKey::new(1, 100);
    c.put(k, block(b"shared"));
    let h1 = c.get(&k).unwrap();
    let h2 = c.get(&k).unwrap();
    assert!(
        Arc::ptr_eq(&h1, &h2),
        "cache hits hand out Arc clones, not copies"
    );
}

#[test]
fn hits_and_misses_are_counted() {
    let c = LruBlockCache::new(4);
    let k = BlockKey::new(1, 0);
    c.put(k, block(b"v"));
    c.get(&k);
    c.get(&k);
    c.get(&BlockKey::new(99, 0));
    assert_eq!(c.hits(), 2);
    assert_eq!(c.misses(), 1);
}

#[test]
fn distinct_keys_share_cache_when_under_capacity() {
    let c = LruBlockCache::new(4);
    for i in 0..4 {
        c.put(BlockKey::new(i, 0), block(&[i as u8]));
    }
    for i in 0..4 {
        assert!(c.get(&BlockKey::new(i, 0)).is_some());
    }
    assert_eq!(c.len(), 4);
}
