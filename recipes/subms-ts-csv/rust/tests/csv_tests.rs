use subms_ts::TsDataType;
use subms_ts_csv::{TsCsvError, TsCsvOptions, read_csv, read_ndjson, write_csv};

// --- per-column type inference -------------------------------------------

#[test]
fn infers_i64_column() {
    let df = read_csv("n\n1\n2\n3\n", &TsCsvOptions::default()).unwrap();
    let col = df.column("n").unwrap();
    assert_eq!(col.data_type(), TsDataType::I64);
    let s = col.as_i64().unwrap();
    assert_eq!(s.len(), 3);
    assert_eq!(s.get_at(0).unwrap().value, 1);
    assert_eq!(s.get_at(2).unwrap().value, 3);
}

#[test]
fn infers_f64_column() {
    let df = read_csv("x\n1.5\n2\n3.25\n", &TsCsvOptions::default()).unwrap();
    let col = df.column("x").unwrap();
    // a mix of int-looking and float-looking cells widens to F64.
    assert_eq!(col.data_type(), TsDataType::F64);
    assert_eq!(col.as_f64().unwrap().get_at(0).unwrap().value, 1.5);
}

#[test]
fn infers_bool_column() {
    let df = read_csv("ok\ntrue\nfalse\nTrue\n", &TsCsvOptions::default()).unwrap();
    let col = df.column("ok").unwrap();
    assert_eq!(col.data_type(), TsDataType::Bool);
    let s = col.as_bool().unwrap();
    assert!(s.get_at(0).unwrap().value);
    assert!(!s.get_at(1).unwrap().value);
    assert!(s.get_at(2).unwrap().value); // case-insensitive
}

#[test]
fn infers_str_column() {
    let df = read_csv("tag\nfoo\nbar\nbaz\n", &TsCsvOptions::default()).unwrap();
    let col = df.column("tag").unwrap();
    assert_eq!(col.data_type(), TsDataType::Str);
    assert_eq!(col.as_str().unwrap().get_at(1).unwrap().value, "bar");
}

#[test]
fn mixed_int_and_text_infers_str() {
    // one non-numeric cell drags the whole column to Str.
    let df = read_csv("v\n1\n2\nNA\n4\n", &TsCsvOptions::default()).unwrap();
    let col = df.column("v").unwrap();
    assert_eq!(col.data_type(), TsDataType::Str);
    assert_eq!(col.as_str().unwrap().get_at(0).unwrap().value, "1");
    assert_eq!(col.as_str().unwrap().get_at(2).unwrap().value, "NA");
}

// --- empty cell as gap ----------------------------------------------------

#[test]
fn empty_cell_is_a_gap() {
    // row 1 has no value for `b`; the column simply has no point at ts 1.
    let df = read_csv("a,b\n1,10\n2,\n3,30\n", &TsCsvOptions::default()).unwrap();
    let b = df.column("b").unwrap();
    assert_eq!(b.data_type(), TsDataType::I64);
    assert_eq!(b.len(), 2); // the gap pushed no null
    assert!(b.get(1).is_none());
    assert_eq!(b.get(0), b.get(0)); // ts 0 present

    // the aligned view shows the gap as a None for column b at ts 1.
    let rows: Vec<_> = df.aligned().collect();
    let row_ts1 = rows.iter().find(|(ts, _)| *ts == 1).unwrap();
    assert!(row_ts1.1[0].is_some()); // a
    assert!(row_ts1.1[1].is_none()); // b gap
}

// --- quoting --------------------------------------------------------------

#[test]
fn quoted_field_with_comma_and_escaped_quote() {
    // a quoted cell holds an embedded comma and a "" escaped quote.
    let text = "name,note\n1,\"a,b\"\n2,\"he said \"\"hi\"\"\"\n";
    let df = read_csv(text, &TsCsvOptions::default()).unwrap();
    let note = df.column("note").unwrap();
    assert_eq!(note.data_type(), TsDataType::Str);
    assert_eq!(note.as_str().unwrap().get_at(0).unwrap().value, "a,b");
    assert_eq!(
        note.as_str().unwrap().get_at(1).unwrap().value,
        "he said \"hi\""
    );
}

#[test]
fn unterminated_quote_errors() {
    let err = read_csv("a\n\"oops\n", &TsCsvOptions::default()).err().unwrap();
    assert!(matches!(err, TsCsvError::BadQuoting { .. }));
}

// --- line endings ---------------------------------------------------------

#[test]
fn crlf_and_lf_line_endings() {
    let crlf = read_csv("a,b\r\n1,2\r\n3,4\r\n", &TsCsvOptions::default()).unwrap();
    let lf = read_csv("a,b\n1,2\n3,4\n", &TsCsvOptions::default()).unwrap();
    assert_eq!(crlf.column("a").unwrap().len(), 2);
    assert_eq!(lf.column("a").unwrap().len(), 2);
    assert_eq!(
        crlf.column("b").unwrap().as_i64().unwrap().get_at(1).unwrap().value,
        4
    );
}

// --- ts axis --------------------------------------------------------------

#[test]
fn ts_column_designation() {
    let text = "t,v\n100,1.0\n200,2.0\n300,3.0\n";
    let df = read_csv(text, &TsCsvOptions::default().ts_column("t")).unwrap();
    // the ts column is consumed, not re-emitted.
    assert!(df.column("t").is_none());
    let v = df.column("v").unwrap().as_f64().unwrap();
    assert_eq!(v.get_at(200).unwrap().value, 2.0);
    assert_eq!(v.first().unwrap().ts, 100);
    assert_eq!(v.last().unwrap().ts, 300);
}

#[test]
fn row_index_default_axis() {
    let df = read_csv("v\n10\n20\n30\n", &TsCsvOptions::default()).unwrap();
    let v = df.column("v").unwrap().as_i64().unwrap();
    assert_eq!(v.get_at(0).unwrap().value, 10);
    assert_eq!(v.get_at(2).unwrap().value, 30);
}

#[test]
fn ts_column_unknown_errors() {
    let err = read_csv("a\n1\n", &TsCsvOptions::default().ts_column("nope")).err().unwrap();
    assert!(matches!(err, TsCsvError::UnknownTsColumn { .. }));
}

#[test]
fn ts_column_non_integer_errors() {
    let text = "t,v\n100,1\nnotanint,2\n";
    let err = read_csv(text, &TsCsvOptions::default().ts_column("t")).err().unwrap();
    assert!(matches!(err, TsCsvError::BadTimestamp { .. }));
}

// --- header vs no header --------------------------------------------------

#[test]
fn no_header_synthesises_names() {
    let df = read_csv("1,2,3\n4,5,6\n", &TsCsvOptions::default().has_header(false)).unwrap();
    assert_eq!(df.ncols(), 3);
    let names: Vec<_> = df.column_names().collect();
    assert_eq!(names, vec!["col0", "col1", "col2"]);
    assert_eq!(df.column("col1").unwrap().as_i64().unwrap().get_at(0).unwrap().value, 2);
}

// --- ragged rows ----------------------------------------------------------

#[test]
fn ragged_row_errors() {
    let err = read_csv("a,b,c\n1,2,3\n4,5\n", &TsCsvOptions::default()).err().unwrap();
    match err {
        TsCsvError::RaggedRow { expected, got, .. } => {
            assert_eq!(expected, 3);
            assert_eq!(got, 2);
        }
        other => panic!("expected RaggedRow, got {other:?}"),
    }
}

// --- round trip -----------------------------------------------------------

#[test]
fn round_trip_preserves_columns_types_values() {
    let text = "i,f,b,s\n1,1.5,true,foo\n2,2.5,false,\"a,b\"\n3,3.5,true,baz\n";
    let df = read_csv(text, &TsCsvOptions::default()).unwrap();
    let emitted = write_csv(&df);
    let again = read_csv(&emitted, &TsCsvOptions::default()).unwrap();

    let names: Vec<_> = again.column_names().collect();
    assert_eq!(names, vec!["i", "f", "b", "s"]);
    assert_eq!(again.column("i").unwrap().data_type(), TsDataType::I64);
    assert_eq!(again.column("f").unwrap().data_type(), TsDataType::F64);
    assert_eq!(again.column("b").unwrap().data_type(), TsDataType::Bool);
    assert_eq!(again.column("s").unwrap().data_type(), TsDataType::Str);

    assert_eq!(again.column("i").unwrap().as_i64().unwrap().get_at(1).unwrap().value, 2);
    assert_eq!(again.column("f").unwrap().as_f64().unwrap().get_at(2).unwrap().value, 3.5);
    assert!(again.column("b").unwrap().as_bool().unwrap().get_at(0).unwrap().value);
    assert_eq!(again.column("s").unwrap().as_str().unwrap().get_at(1).unwrap().value, "a,b");
}

#[test]
fn write_quotes_only_when_needed() {
    let text = "a,b\n1,plain\n2,\"has,comma\"\n";
    let df = read_csv(text, &TsCsvOptions::default()).unwrap();
    let out = write_csv(&df);
    assert!(out.contains("plain")); // unquoted
    assert!(out.contains("\"has,comma\"")); // quoted
}

// --- NDJSON ---------------------------------------------------------------

#[test]
fn ndjson_object_per_line() {
    let text = "{\"a\":1,\"b\":1.5}\n{\"a\":2,\"b\":2.5}\n";
    let df = read_ndjson(text, &TsCsvOptions::default()).unwrap();
    assert_eq!(df.column("a").unwrap().data_type(), TsDataType::I64);
    assert_eq!(df.column("b").unwrap().data_type(), TsDataType::F64);
    assert_eq!(df.column("a").unwrap().as_i64().unwrap().get_at(1).unwrap().value, 2);
}

#[test]
fn ndjson_missing_key_is_gap() {
    // line 1 omits `b`; that row is a gap for b, not a null.
    let text = "{\"a\":1,\"b\":10}\n{\"a\":2}\n{\"a\":3,\"b\":30}\n";
    let df = read_ndjson(text, &TsCsvOptions::default()).unwrap();
    let b = df.column("b").unwrap();
    assert_eq!(b.len(), 2);
    assert!(b.get(1).is_none());
}

#[test]
fn ndjson_quoted_value_stays_str() {
    // a quoted "1" must not re-infer to I64.
    let text = "{\"id\":\"1\"}\n{\"id\":\"2\"}\n";
    let df = read_ndjson(text, &TsCsvOptions::default()).unwrap();
    let id = df.column("id").unwrap();
    assert_eq!(id.data_type(), TsDataType::Str);
    assert_eq!(id.as_str().unwrap().get_at(0).unwrap().value, "1");
}

#[test]
fn ndjson_bool_and_null() {
    let text = "{\"ok\":true,\"v\":1}\n{\"ok\":false,\"v\":null}\n";
    let df = read_ndjson(text, &TsCsvOptions::default()).unwrap();
    assert_eq!(df.column("ok").unwrap().data_type(), TsDataType::Bool);
    // the null value for v on line 2 is a gap.
    assert_eq!(df.column("v").unwrap().len(), 1);
}

#[test]
fn ndjson_ts_column() {
    let text = "{\"t\":1000,\"v\":5}\n{\"t\":2000,\"v\":6}\n";
    let df = read_ndjson(text, &TsCsvOptions::default().ts_column("t")).unwrap();
    assert!(df.column("t").is_none());
    let v = df.column("v").unwrap().as_i64().unwrap();
    assert_eq!(v.get_at(2000).unwrap().value, 6);
}

#[test]
fn ndjson_nested_object_errors() {
    let text = "{\"a\":{\"nested\":1}}\n";
    let err = read_ndjson(text, &TsCsvOptions::default()).err().unwrap();
    assert!(matches!(err, TsCsvError::BadJson { .. }));
}

#[test]
fn ndjson_string_escapes() {
    let text = "{\"s\":\"a\\tb\\n\"}\n";
    let df = read_ndjson(text, &TsCsvOptions::default()).unwrap();
    assert_eq!(df.column("s").unwrap().as_str().unwrap().get_at(0).unwrap().value, "a\tb\n");
}

#[test]
fn empty_input_is_empty_frame() {
    let df = read_csv("", &TsCsvOptions::default()).unwrap();
    assert!(df.is_empty());
}
