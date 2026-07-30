use super::*;

#[test]
fn round_trip_empty_tree() {
    let t: Art<u32> = Art::new();
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<u32> = parse(&mut cursor).unwrap();
    assert_eq!(restored.len(), 0);
    assert!(restored.is_empty());
}

#[test]
fn round_trip_preserves_all_insertions() {
    let mut t: Art<u32> = Art::new();
    for i in 0..200u32 {
        let key = format!("key{i:04}");
        t.insert(key.as_bytes(), i);
    }
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<u32> = parse(&mut cursor).unwrap();
    assert_eq!(restored.len(), 200);
    for i in 0..200u32 {
        let key = format!("key{i:04}");
        assert_eq!(restored.get(key.as_bytes()).copied(), Some(i), "key {key}");
    }
}

#[test]
fn round_trip_with_node_growth() {
    // 256 distinct first bytes force the root to grow Small -> Full.
    let mut t: Art<u32> = Art::new();
    for i in 0..=255u8 {
        t.insert(&[i, 7, 9], i as u32);
    }
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<u32> = parse(&mut cursor).unwrap();
    assert_eq!(restored.len(), 256);
    for i in 0..=255u8 {
        assert_eq!(restored.get(&[i, 7, 9]).copied(), Some(i as u32));
    }
}

#[test]
fn round_trip_with_string_values() {
    let mut t: Art<String> = Art::new();
    t.insert(b"a", "alpha".to_string());
    t.insert(b"b", "beta".to_string());
    t.insert(b"long-key-with-many-bytes", "gamma".to_string());
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<String> = parse(&mut cursor).unwrap();
    assert_eq!(restored.get(b"a").map(|s| s.as_str()), Some("alpha"));
    assert_eq!(restored.get(b"b").map(|s| s.as_str()), Some("beta"));
    assert_eq!(
        restored
            .get(b"long-key-with-many-bytes")
            .map(|s| s.as_str()),
        Some("gamma")
    );
}

#[test]
fn round_trip_binary_keys_with_zeros() {
    let mut t: Art<u32> = Art::new();
    t.insert(&[0, 0, 0], 1);
    t.insert(&[0, 1, 2], 2);
    t.insert(b"", 99); // empty-key carries a value at the root
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<u32> = parse(&mut cursor).unwrap();
    assert_eq!(restored.get(&[0, 0, 0]).copied(), Some(1));
    assert_eq!(restored.get(&[0, 1, 2]).copied(), Some(2));
    assert_eq!(restored.get(b"").copied(), Some(99));
    assert_eq!(restored.len(), 3);
}

#[test]
fn round_trip_vec_u8_values() {
    let mut t: Art<Vec<u8>> = Art::new();
    t.insert(b"x", vec![1, 2, 3]);
    t.insert(b"y", vec![]);
    t.insert(b"zzz", vec![9; 40]);
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<Vec<u8>> = parse(&mut cursor).unwrap();
    assert_eq!(restored.get(b"x").cloned(), Some(vec![1, 2, 3]));
    assert_eq!(restored.get(b"y").cloned(), Some(vec![]));
    assert_eq!(restored.get(b"zzz").cloned(), Some(vec![9; 40]));
}

#[test]
fn round_trip_u64_values() {
    let mut t: Art<u64> = Art::new();
    t.insert(b"a", 1);
    t.insert(b"b", u64::MAX);
    t.insert(b"cc", 1 << 40);
    let mut buf = Vec::new();
    write_to(&t, &mut buf).unwrap();
    let mut cursor = &buf[..];
    let restored: Art<u64> = parse(&mut cursor).unwrap();
    assert_eq!(restored.get(b"a").copied(), Some(1));
    assert_eq!(restored.get(b"b").copied(), Some(u64::MAX));
    assert_eq!(restored.get(b"cc").copied(), Some(1 << 40));
}

#[test]
fn u32_codec_rejects_wrong_length() {
    let data = [0u8; 3];
    let mut cur = &data[..];
    let err = <u32 as ArtCodec>::read_value(&mut cur, 3).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn u64_codec_rejects_wrong_length() {
    let data = [0u8; 4];
    let mut cur = &data[..];
    let err = <u64 as ArtCodec>::read_value(&mut cur, 4).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn string_codec_rejects_invalid_utf8() {
    let data = [0xff, 0xfe, 0xfd];
    let mut cur = &data[..];
    let err = <String as ArtCodec>::read_value(&mut cur, 3).unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::InvalidData);
}

#[test]
fn bad_tag_is_rejected() {
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&VERSION.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u64.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes()); // root prefix_len = 0
    buf.push(0xff); // invalid shape tag
    let mut cursor = &buf[..];
    match parse::<u32, _>(&mut cursor) {
        Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
        Ok(_) => panic!("parse should have rejected bad tag"),
    }
}

#[test]
fn bad_magic_is_rejected() {
    let bad = b"\x00\x00\x00\x00\x00\x01\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00".to_vec();
    let mut cursor = &bad[..];
    match parse::<u32, _>(&mut cursor) {
        Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
        Ok(_) => panic!("parse should have rejected bad magic"),
    }
}

#[test]
fn bad_version_is_rejected() {
    let mut buf = Vec::new();
    buf.extend_from_slice(MAGIC);
    buf.extend_from_slice(&999u16.to_be_bytes());
    buf.extend_from_slice(&0u16.to_be_bytes());
    buf.extend_from_slice(&0u64.to_be_bytes());
    let mut cursor = &buf[..];
    match parse::<u32, _>(&mut cursor) {
        Err(e) => assert_eq!(e.kind(), io::ErrorKind::InvalidData),
        Ok(_) => panic!("parse should have rejected bad version"),
    }
}
