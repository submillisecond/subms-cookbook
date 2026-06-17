use subms_ts::{TsColumn, TsSeries};
use subms_ts_categorical::{TsDictColumn, TsStringInterner, encode_str_column};

// ---------- TsStringInterner ----------

#[test]
fn intern_same_string_same_id() {
    let mut it = TsStringInterner::new();
    let a = it.intern("AAPL");
    let b = it.intern("AAPL");
    assert_eq!(a, b);
    assert_eq!(it.len(), 1);
}

#[test]
fn intern_distinct_strings_distinct_ids() {
    let mut it = TsStringInterner::new();
    let a = it.intern("AAPL");
    let b = it.intern("MSFT");
    assert_ne!(a, b);
    assert_eq!(it.len(), 2);
}

#[test]
fn intern_ids_dense_from_zero_in_first_seen_order() {
    let mut it = TsStringInterner::new();
    assert_eq!(it.intern("c"), 0);
    assert_eq!(it.intern("a"), 1);
    assert_eq!(it.intern("b"), 2);
    // repeats do not advance the counter.
    assert_eq!(it.intern("a"), 1);
    assert_eq!(it.intern("c"), 0);
    assert_eq!(it.intern("d"), 3);
    assert_eq!(it.len(), 4);
}

#[test]
fn resolve_round_trips_every_id() {
    let mut it = TsStringInterner::new();
    let ids: Vec<(u32, &str)> = ["x", "y", "z"].iter().map(|s| (it.intern(s), *s)).collect();
    for (id, s) in ids {
        assert_eq!(it.resolve(id), Some(s));
    }
    assert_eq!(it.resolve(999), None);
}

#[test]
fn contains_and_get_reflect_membership() {
    let mut it = TsStringInterner::new();
    assert!(!it.contains("AAPL"));
    assert_eq!(it.get("AAPL"), None);
    let id = it.intern("AAPL");
    assert!(it.contains("AAPL"));
    assert_eq!(it.get("AAPL"), Some(id));
    // get does not assign.
    assert_eq!(it.get("MSFT"), None);
    assert_eq!(it.len(), 1);
}

#[test]
fn interner_empty_state() {
    let it = TsStringInterner::with_capacity(8);
    assert!(it.is_empty());
    assert_eq!(it.len(), 0);
    assert_eq!(it.strings().len(), 0);
}

#[test]
fn interner_strings_are_in_id_order() {
    let mut it = TsStringInterner::new();
    it.intern("first");
    it.intern("second");
    it.intern("first");
    assert_eq!(it.strings(), &["first".to_string(), "second".to_string()]);
}

// ---------- TsDictColumn ----------

#[test]
fn encode_string_series_codes_and_cardinality() {
    let mut s = TsSeries::<String>::new();
    for (i, v) in ["a", "b", "a", "c", "b"].iter().enumerate() {
        s.push(i as i64, v.to_string()).unwrap();
    }
    let col = TsDictColumn::encode(&s);
    assert_eq!(col.len(), 5);
    assert_eq!(col.cardinality(), 3);
    assert_eq!(col.dict(), &["a".to_string(), "b".to_string(), "c".to_string()]);
    // codes follow input order with dense first-seen ids.
    assert_eq!(col.codes(), &[0, 1, 0, 2, 1]);
}

#[test]
fn decode_at_and_to_series_round_trip_values() {
    let original = ["x", "y", "x", "z", "y", "x"];
    let col = TsDictColumn::from_strs(original);
    for (i, v) in original.iter().enumerate() {
        assert_eq!(col.decode_at(i), Some(*v));
    }
    assert_eq!(col.decode_at(original.len()), None);
    let series = col.to_series();
    let decoded: Vec<String> = series.iter().map(|p| p.value).collect();
    let expect: Vec<String> = original.iter().map(|s| s.to_string()).collect();
    assert_eq!(decoded, expect);
    assert_eq!(col.to_vec(), expect);
}

#[test]
fn high_duplication_column_yields_small_dictionary() {
    let symbols = ["AAPL", "MSFT", "GOOG", "AMZN"];
    let values: Vec<&str> = (0..1000).map(|i| symbols[i % symbols.len()]).collect();
    let col = TsDictColumn::from_strs(values);
    assert_eq!(col.len(), 1000);
    assert_eq!(col.cardinality(), 4);
    assert_eq!(col.codes().len(), 1000);
}

#[test]
fn equal_strings_share_a_code() {
    let col = TsDictColumn::from_strs(["AAPL", "MSFT", "AAPL", "AAPL", "MSFT"]);
    let codes = col.codes();
    // all AAPL rows carry one code, all MSFT rows another.
    assert_eq!(codes[0], codes[2]);
    assert_eq!(codes[0], codes[3]);
    assert_eq!(codes[1], codes[4]);
    assert_ne!(codes[0], codes[1]);
}

#[test]
fn empty_column() {
    let col = TsDictColumn::from_strs(Vec::<String>::new());
    assert!(col.is_empty());
    assert_eq!(col.len(), 0);
    assert_eq!(col.cardinality(), 0);
    assert_eq!(col.decode_at(0), None);
    assert!(col.to_series().is_empty());
    assert!(col.to_vec().is_empty());
}

#[test]
fn single_distinct_value() {
    let col = TsDictColumn::from_strs(["only", "only", "only"]);
    assert_eq!(col.len(), 3);
    assert_eq!(col.cardinality(), 1);
    assert_eq!(col.codes(), &[0, 0, 0]);
    assert_eq!(col.lookup(0), Some("only"));
    assert_eq!(col.lookup(1), None);
}

#[test]
fn lookup_resolves_dictionary_codes() {
    let col = TsDictColumn::from_strs(["red", "green", "blue", "green"]);
    assert_eq!(col.lookup(0), Some("red"));
    assert_eq!(col.lookup(1), Some("green"));
    assert_eq!(col.lookup(2), Some("blue"));
    assert_eq!(col.lookup(3), None);
}

// ---------- bridge to TsColumn ----------

#[test]
fn encode_str_column_bridges_a_frame_column() {
    let mut s = TsSeries::<String>::new();
    for (i, v) in ["EU", "US", "EU"].iter().enumerate() {
        s.push(i as i64, v.to_string()).unwrap();
    }
    let col = TsColumn::Str(s);
    let dict = encode_str_column(&col).expect("string column encodes");
    assert_eq!(dict.cardinality(), 2);
    assert_eq!(dict.codes(), &[0, 1, 0]);
}

#[test]
fn encode_str_column_rejects_non_string_column() {
    let mut s = TsSeries::<f64>::new();
    s.push(0, 1.0).unwrap();
    let col = TsColumn::F64(s);
    assert!(encode_str_column(&col).is_none());
}
