use subms_ts::{
    Curve, Ohlc, TsAgg, TsAttrs, TsCodec, TsCollection, TsColumn, TsDataFrame, TsDataType, TsError,
    TsJsonCodec, TsPanel, TsPanelGroup, TsPanelMetadata, TsSeries, TsSeriesMetadata,
    TsTimestampStyle, TsValue, TsValueKind,
};

// ---------- value types ----------

#[test]
fn ohlc_series_time_queries() {
    let mut s: TsSeries<Ohlc> = TsSeries::new();
    s.push(1, Ohlc::new(1.0, 2.0, 0.5, 1.5, 100.0)).unwrap();
    s.push(2, Ohlc::new(1.5, 2.5, 1.0, 2.0, 120.0)).unwrap();
    assert_eq!(s.len(), 2);
    assert_eq!(s.nearest_before(2).unwrap().value.close, 2.0);
    // extract a scalar field to aggregate (matches stochbook's `close` op)
    let closes: TsSeries<f64> = TsSeries::from_points(
        s.iter()
            .map(|p| subms_ts::TsPoint::new(p.ts, p.value.close))
            .collect(),
    )
    .unwrap();
    assert_eq!(closes.max(), Some(2.0));
}

#[test]
fn ohlc_rejects_nan_field() {
    let mut s: TsSeries<Ohlc> = TsSeries::new();
    assert!(
        s.push(1, Ohlc::new(1.0, f64::NAN, 0.5, 1.5, 100.0))
            .is_err()
    );
    assert_eq!(s.len(), 0);
}

#[test]
fn curve_series_and_presence() {
    let mut s: TsSeries<Curve> = TsSeries::new();
    s.push(1, Curve::new(vec![1.0, 2.0, 5.0], vec![0.01, 0.015, 0.02]))
        .unwrap();
    assert_eq!(s.first().unwrap().value.values.len(), 3);
    assert!(!Curve::new(vec![1.0], vec![f64::NAN]).ts_is_present());
}

#[test]
fn tsvalue_null_rejected_but_nested_null_ok() {
    let mut s: TsSeries<TsValue> = TsSeries::new();
    assert!(s.push(1, TsValue::Null).is_err());
    let mut map = std::collections::BTreeMap::new();
    map.insert("a".to_string(), TsValue::Null); // nested null is intentional
    assert!(s.push(1, TsValue::Map(map)).is_ok());
    assert_eq!(s.len(), 1);
}

// ---------- metadata ----------

#[test]
fn attrs_normalise_and_reject_non_ascii() {
    let mut a = TsAttrs::new();
    a.insert("  Bar-Interval ", "  1M ").unwrap();
    assert_eq!(a.get("bar-interval"), Some("1m"));
    assert_eq!(a.get("BAR-INTERVAL"), Some("1m")); // query is normalised too
    assert!(a.matches("Bar-Interval", "1m"));
    assert!(a.insert("naïve", "x").is_err()); // non-ASCII key rejected
}

#[test]
fn metadata_builder_and_tag_match() {
    let m = TsSeriesMetadata::new(7, "close.aapl")
        .with_tag("symbol", "aapl")
        .with_tag("field", "close");
    assert_eq!(m.id, 7);
    let mut want = std::collections::BTreeMap::new();
    want.insert("symbol".to_string(), "aapl".to_string());
    assert!(m.has_tags(&want));
}

// ---------- codec ----------

#[test]
fn json_codec_roundtrip_epoch_nanos() {
    let mut s = TsSeries::<f64>::new();
    for (t, v) in [(1_000i64, 1.5), (2_000, 2.0), (3_000, 2.5)] {
        s.push(t, v).unwrap();
    }
    let codec = TsJsonCodec::new();
    let bytes = codec.encode(&s);
    let back = codec.decode(&bytes).unwrap();
    assert_eq!(back.len(), 3);
    assert_eq!(back.get_at(2_000).unwrap().value, 2.0);
    assert_eq!(
        back.iter().map(|p| p.ts).collect::<Vec<_>>(),
        vec![1_000, 2_000, 3_000]
    );
}

#[test]
fn json_codec_millis_roundtrip_and_iso_encode() {
    let mut s = TsSeries::<f64>::new();
    s.push(1_000_000_000, 1.0).unwrap(); // 1 second in nanos
    let millis = TsJsonCodec::new().with_style(TsTimestampStyle::EpochMillis);
    let back = millis.decode(&millis.encode(&s)).unwrap();
    assert_eq!(back.first().unwrap().ts, 1_000_000_000);

    let iso = TsJsonCodec::new().with_style(TsTimestampStyle::Iso8601);
    let text = String::from_utf8(iso.encode(&s)).unwrap();
    assert!(
        text.contains("1970-01-01T00:00:01.000000000Z"),
        "got: {text}"
    );
    // ISO decode is unsupported in 0.6
    assert!(iso.decode(text.as_bytes()).is_err());
}

#[test]
fn json_codec_includes_name_from_metadata() {
    let s = TsSeries::<f64>::new().with_metadata(TsSeriesMetadata::new(1, "trades.aapl"));
    let text = String::from_utf8(TsJsonCodec::new().encode(&s)).unwrap();
    assert!(text.contains("\"name\":\"trades.aapl\""));
}

// ---------- collection ----------

#[test]
fn collection_register_push_lookup() {
    let mut c: TsCollection<f64> = TsCollection::new();
    let id = c
        .register(TsSeriesMetadata::new(1, "cpu").with_tag("host", "a"))
        .unwrap();
    c.push(id, 10, 0.5).unwrap();
    c.push(id, 20, 0.7).unwrap();
    assert_eq!(c.len(), 1);
    assert_eq!(c.by_name("cpu").unwrap().len(), 2);
    assert_eq!(c.by_tag("host", "a").count(), 1);
    assert_eq!(c.by_tag("host", "z").count(), 0);
    assert!(matches!(
        c.register(TsSeriesMetadata::new(1, "dup")),
        Err(subms_ts::TsCollectionError::DuplicateId(1))
    ));
    assert!(c.push(99, 1, 1.0).is_err());
}

#[test]
fn collection_aggregate_at_by_tag() {
    let mut c: TsCollection<f64> = TsCollection::new();
    for (id, host, v) in [(1u64, "a", 1.0), (2, "a", 3.0), (3, "b", 9.0)] {
        c.register(TsSeriesMetadata::new(id, format!("s{id}")).with_tag("host", host))
            .unwrap();
        c.push(id, 100, v).unwrap();
    }
    // latest value as-of ts across host=a series: 1.0 + 3.0
    assert_eq!(
        c.aggregate_at_by_tag("host", "a", 1_000, TsAgg::Sum),
        Some(4.0)
    );
    assert_eq!(
        c.aggregate_at_by_tag("host", "a", 1_000, TsAgg::Max),
        Some(3.0)
    );
    assert_eq!(c.aggregate_at(1_000, TsAgg::Count), Some(3.0));
}

#[test]
fn collection_delete_and_evict_by_tag() {
    let mut c: TsCollection<f64> = TsCollection::new();
    for (id, env) in [(1u64, "prod"), (2, "prod"), (3, "dev")] {
        c.register(TsSeriesMetadata::new(id, format!("s{id}")).with_tag("env", env))
            .unwrap();
        c.push(id, 1, 1.0).unwrap();
    }
    assert_eq!(c.delete_range_by_tag("env", "prod", 0, 10), 2);
    let evicted = c.evict_by_tag("env", "prod");
    assert_eq!(evicted.len(), 2);
    assert_eq!(c.len(), 1);
}

// ---------- panel (homogeneous) ----------

#[test]
fn panel_slots_groups_and_aligned() {
    let mut f: TsPanel<f64> = TsPanel::new(TsPanelMetadata::new("ohlcv.aapl.1m"));
    let mut open = TsSeries::new();
    let mut close = TsSeries::new();
    open.push(1, 10.0).unwrap();
    open.push(3, 12.0).unwrap();
    close.push(1, 10.5).unwrap();
    close.push(2, 11.0).unwrap();
    f.add_series("open", open);
    f.add_series("close", close);
    f.add_group(TsPanelGroup {
        name: "price".into(),
        series_names: vec!["open".into(), "close".into()],
    });

    assert_eq!(f.len(), 2);
    assert_eq!(f.slot_names().collect::<Vec<_>>(), vec!["open", "close"]);
    assert_eq!(f.series_in_group("price").count(), 2);

    // aligned rows in ts order; None where a slot has no point at that ts
    let rows: Vec<(i64, Vec<Option<f64>>)> = f.aligned().collect();
    assert_eq!(
        rows,
        vec![
            (1, vec![Some(10.0), Some(10.5)]),
            (2, vec![None, Some(11.0)]),
            (3, vec![Some(12.0), None]),
        ]
    );
}

#[test]
fn panel_delete_and_drop() {
    let mut f: TsPanel<f64> = TsPanel::new(TsPanelMetadata::new("f"));
    let mut a = TsSeries::new();
    a.push(1, 1.0).unwrap();
    a.push(2, 2.0).unwrap();
    f.add_series("a", a);
    assert_eq!(f.delete_range("a", 1, 1), 1);
    assert_eq!(f.series("a").unwrap().len(), 1);
    assert!(f.drop("a").is_some());
    assert!(f.is_empty());
}

// ---------- dataframe (heterogeneous) ----------

fn f64_series(points: &[(i64, f64)]) -> TsSeries<f64> {
    let mut s = TsSeries::new();
    for &(ts, v) in points {
        s.push(ts, v).unwrap();
    }
    s
}

#[test]
fn column_f64_variant() {
    let col = TsColumn::F64(f64_series(&[(1, 10.0), (2, 12.5)]));
    assert_eq!(col.data_type(), TsDataType::F64);
    assert_eq!(col.len(), 2);
    assert!(!col.is_empty());
    assert!(col.as_f64().is_some());
    assert!(col.as_i64().is_none());
    assert_eq!(col.get(2), Some(TsValue::F64(12.5)));
    assert_eq!(col.get(3), None);
}

#[test]
fn column_i64_variant() {
    let mut s = TsSeries::<i64>::new();
    s.push(1, 100).unwrap();
    s.push(2, 250).unwrap();
    let col = TsColumn::I64(s);
    assert_eq!(col.data_type(), TsDataType::I64);
    assert_eq!(col.len(), 2);
    assert!(col.as_i64().is_some());
    assert!(col.as_f64().is_none());
    assert_eq!(col.get(1), Some(TsValue::I64(100)));
}

#[test]
fn column_bool_variant() {
    let mut s = TsSeries::<bool>::new();
    s.push(1, true).unwrap();
    s.push(2, false).unwrap();
    let col = TsColumn::Bool(s);
    assert_eq!(col.data_type(), TsDataType::Bool);
    assert_eq!(col.len(), 2);
    assert!(col.as_bool().is_some());
    assert!(col.as_str().is_none());
    assert_eq!(col.get(2), Some(TsValue::Bool(false)));
}

#[test]
fn column_str_variant() {
    let mut s = TsSeries::<String>::new();
    s.push(1, "AAPL".to_string()).unwrap();
    s.push(2, "MSFT".to_string()).unwrap();
    let col = TsColumn::Str(s);
    assert_eq!(col.data_type(), TsDataType::Str);
    assert_eq!(col.len(), 2);
    assert!(col.as_str().is_some());
    assert!(col.as_value().is_none());
    assert_eq!(col.get(1), Some(TsValue::Str("AAPL".to_string())));
}

#[test]
fn column_value_variant() {
    let mut s = TsSeries::<TsValue>::new();
    s.push(1, TsValue::I64(7)).unwrap();
    s.push(2, TsValue::Str("x".into())).unwrap();
    let col = TsColumn::Value(s);
    assert_eq!(col.data_type(), TsDataType::Value);
    assert_eq!(col.len(), 2);
    assert!(col.as_value().is_some());
    assert!(col.as_f64().is_none());
    assert_eq!(col.get(2), Some(TsValue::Str("x".into())));
}

#[test]
fn column_empty_is_empty() {
    let col = TsColumn::F64(TsSeries::new());
    assert!(col.is_empty());
    assert_eq!(col.len(), 0);
    assert_eq!(col.get(1), None);
}

fn mixed_frame() -> TsDataFrame {
    let mut price = TsSeries::<f64>::new();
    price.push(1, 100.0).unwrap();
    price.push(2, 101.5).unwrap();
    let mut symbol = TsSeries::<String>::new();
    symbol.push(1, "AAPL".into()).unwrap();
    symbol.push(2, "AAPL".into()).unwrap();
    let mut volume = TsSeries::<i64>::new();
    volume.push(1, 500).unwrap();
    volume.push(2, 700).unwrap();
    TsDataFrame::new()
        .with_column("price", TsColumn::F64(price))
        .with_column("symbol", TsColumn::Str(symbol))
        .with_column("volume", TsColumn::I64(volume))
}

#[test]
fn dataframe_build_and_access() {
    let df = mixed_frame();
    assert_eq!(df.ncols(), 3);
    assert!(!df.is_empty());
    assert_eq!(
        df.column_names().collect::<Vec<_>>(),
        vec!["price", "symbol", "volume"]
    );
    assert_eq!(df.column("price").unwrap().data_type(), TsDataType::F64);
    assert_eq!(df.column("symbol").unwrap().data_type(), TsDataType::Str);
    assert!(df.column("missing").is_none());
}

#[test]
fn dataframe_schema_is_derived() {
    let df = mixed_frame();
    let schema = df.schema();
    assert_eq!(schema.fields.len(), 3);
    assert_eq!(schema.fields[0].name, "price");
    assert_eq!(schema.fields[0].data_type, TsDataType::F64);
    assert_eq!(schema.fields[2].name, "volume");
    assert_eq!(schema.fields[2].data_type, TsDataType::I64);
}

#[test]
fn dataframe_select_projects() {
    let df = mixed_frame();
    let proj = df.select(&["volume", "price"]);
    assert_eq!(
        proj.column_names().collect::<Vec<_>>(),
        vec!["volume", "price"]
    );
    assert!(proj.column("symbol").is_none());
    assert_eq!(
        proj.column("price").unwrap().get(1),
        Some(TsValue::F64(100.0))
    );
}

#[test]
fn dataframe_drop_and_rename() {
    let mut df = mixed_frame();
    let dropped = df.drop("symbol");
    assert!(dropped.is_some());
    assert_eq!(df.ncols(), 2);
    assert!(df.column("symbol").is_none());

    assert!(df.rename("price", "px"));
    assert!(df.column("px").is_some());
    assert!(df.column("price").is_none());
    // renaming onto an existing name fails
    assert!(!df.rename("px", "volume"));
    // renaming a missing column fails
    assert!(!df.rename("nope", "whatever"));
}

#[test]
fn dataframe_duplicate_column_errors() {
    let mut df = TsDataFrame::new();
    df.push_column("a", TsColumn::F64(f64_series(&[(1, 1.0)])))
        .unwrap();
    let err = df
        .push_column("a", TsColumn::I64(TsSeries::new()))
        .unwrap_err();
    assert_eq!(
        err,
        TsError::DuplicateColumn {
            name: "a".to_string()
        }
    );
}

#[test]
fn dataframe_aligned_yields_gap_nulls() {
    // price has points at ts 1 and 3; symbol at ts 1 and 2. The union axis is
    // {1,2,3}; gaps surface as None per the aligned-view null convention.
    let mut price = TsSeries::<f64>::new();
    price.push(1, 100.0).unwrap();
    price.push(3, 102.0).unwrap();
    let mut symbol = TsSeries::<String>::new();
    symbol.push(1, "AAPL".into()).unwrap();
    symbol.push(2, "AAPL".into()).unwrap();
    let df = TsDataFrame::new()
        .with_column("price", TsColumn::F64(price))
        .with_column("symbol", TsColumn::Str(symbol));

    let rows: Vec<(i64, Vec<Option<TsValue>>)> = df.aligned().collect();
    assert_eq!(
        rows,
        vec![
            (
                1,
                vec![Some(TsValue::F64(100.0)), Some(TsValue::Str("AAPL".into()))]
            ),
            (2, vec![None, Some(TsValue::Str("AAPL".into()))]),
            (3, vec![Some(TsValue::F64(102.0)), None]),
        ]
    );
}

#[test]
fn dataframe_empty() {
    let df = TsDataFrame::new();
    assert!(df.is_empty());
    assert_eq!(df.ncols(), 0);
    assert_eq!(df.aligned().count(), 0);
    assert_eq!(df.schema().fields.len(), 0);
}
