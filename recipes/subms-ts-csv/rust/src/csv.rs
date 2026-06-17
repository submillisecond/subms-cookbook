//! Hand-rolled RFC-4180-ish CSV. The tokenizer is a single character-state
//! walk: it splits on the delimiter and on record terminators, honours
//! double-quote quoting with `""` as the embedded-quote escape, and treats a
//! bare CR, a bare LF, and a CRLF pair as one record terminator. There is no
//! buffering layer and no allocation beyond the cell strings themselves.

use subms_ts::TsDataFrame;

use crate::{RawColumn, TsCsvError, TsCsvOptions, assemble};

/// Parse `text` as CSV into a [`TsDataFrame`]. See the crate docs for the
/// header, type-inference, gap, and ts-axis rules.
pub fn read_csv(text: &str, opts: &TsCsvOptions) -> Result<TsDataFrame, TsCsvError> {
    let records = tokenize(text, opts.delimiter)?;
    if records.is_empty() {
        return Ok(TsDataFrame::new());
    }

    let (names, first_data) = if opts.has_header {
        (records[0].clone(), 1usize)
    } else {
        let width = records[0].len();
        ((0..width).map(|i| format!("col{i}")).collect(), 0usize)
    };
    let width = names.len();

    // Resolve the ts axis: a named column index, or the row index 0..N.
    let ts_col = match &opts.ts_column {
        Some(name) => {
            let idx = names
                .iter()
                .position(|n| n == name)
                .ok_or_else(|| TsCsvError::UnknownTsColumn { name: name.clone() })?;
            Some(idx)
        }
        None => None,
    };

    let mut raws: Vec<RawColumn> = (0..width).map(|_| RawColumn::new()).collect();

    for (data_row, record) in records[first_data..].iter().enumerate() {
        if record.len() != width {
            return Err(TsCsvError::RaggedRow {
                row: data_row + first_data,
                expected: width,
                got: record.len(),
            });
        }

        let ts = match ts_col {
            Some(idx) => {
                let cell = &record[idx];
                cell.trim()
                    .parse::<i64>()
                    .map_err(|_| TsCsvError::BadTimestamp {
                        row: data_row + first_data,
                        value: cell.clone(),
                    })?
            }
            None => data_row as i64,
        };

        for (col, cell) in record.iter().enumerate() {
            // the ts column drives the axis; it is not also a value column.
            if Some(col) == ts_col {
                continue;
            }
            // an empty cell is a gap: contribute no point for this column.
            if cell.is_empty() {
                continue;
            }
            raws[col].cells.push((ts, cell.clone()));
        }
    }

    // Drop the ts column's (now empty) slot from the assembled frame.
    let mut out_names = Vec::with_capacity(width);
    let mut out_raws = Vec::with_capacity(width);
    for (i, (name, raw)) in names.into_iter().zip(raws).enumerate() {
        if Some(i) == ts_col {
            continue;
        }
        out_names.push(name);
        out_raws.push(raw);
    }

    Ok(assemble(out_names, out_raws))
}

/// Split CSV text into records of fields. A record terminator is LF, CR, or
/// CRLF; a quoted field may contain the delimiter, a terminator, and `""`.
/// A trailing terminator does not yield a spurious empty record.
fn tokenize(text: &str, delim: char) -> Result<Vec<Vec<String>>, TsCsvError> {
    let mut records: Vec<Vec<String>> = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut in_quotes = false;
    // a field has begun if we have seen any char of it (so a lone "" parses as
    // one empty record only when nothing at all preceded the terminator).
    let mut field_started = false;
    let mut row = 0usize;

    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if in_quotes {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                }
            } else {
                field.push(c);
            }
            continue;
        }

        match c {
            '"' => {
                // a quote may only open a field at its start.
                if !field.is_empty() {
                    return Err(TsCsvError::BadQuoting { row });
                }
                in_quotes = true;
                field_started = true;
            }
            '\r' => {
                // CR or CRLF: consume an optional following LF, end the record.
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                field_started = false;
                row += 1;
            }
            '\n' => {
                record.push(std::mem::take(&mut field));
                records.push(std::mem::take(&mut record));
                field_started = false;
                row += 1;
            }
            ch if ch == delim => {
                record.push(std::mem::take(&mut field));
                field_started = true;
            }
            ch => {
                field.push(ch);
                field_started = true;
            }
        }
    }

    if in_quotes {
        return Err(TsCsvError::BadQuoting { row });
    }

    // a final field with no trailing terminator still belongs to a record.
    if field_started || !field.is_empty() || !record.is_empty() {
        record.push(field);
        records.push(record);
    }

    Ok(records)
}

/// Emit a [`TsDataFrame`] as CSV. The header is the column names; one row per
/// timestamp in the frame's row-aligned view; an empty cell where a column has
/// a gap at that ts. Cells containing the delimiter, a quote, or a newline are
/// double-quoted with `""` escaping. Round-trips with [`read_csv`] for the
/// inferred types (modulo the ts axis, which a written frame carries as the
/// row order, not a named column).
pub fn write_csv(df: &TsDataFrame) -> String {
    let names: Vec<&str> = df.column_names().collect();
    let mut out = String::new();

    for (i, name) in names.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_cell(&mut out, name);
    }
    out.push('\n');

    for (_ts, row) in df.aligned() {
        for (i, cell) in row.iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            if let Some(v) = cell {
                write_cell(&mut out, &value_to_cell(v));
            }
        }
        out.push('\n');
    }

    out
}

/// One cell of a row's value, rendered to the canonical text the reader will
/// re-infer to the same type. A `TsValue::Str` is emitted verbatim; numerics
/// use their plain `Display`; a bool is `true`/`false`.
fn value_to_cell(v: &subms_ts::TsValue) -> String {
    use subms_ts::TsValue;
    match v {
        TsValue::I64(x) => x.to_string(),
        TsValue::F64(x) => x.to_string(),
        TsValue::Bool(b) => b.to_string(),
        TsValue::Str(s) => s.clone(),
        TsValue::Bytes(_) | TsValue::Null | TsValue::Map(_) | TsValue::Array(_) => String::new(),
    }
}

/// Append `cell`, quoting + escaping only when the content forces it.
fn write_cell(out: &mut String, cell: &str) {
    let needs_quote = cell.contains([',', '"', '\n', '\r']);
    if needs_quote {
        out.push('"');
        for ch in cell.chars() {
            if ch == '"' {
                out.push('"');
            }
            out.push(ch);
        }
        out.push('"');
    } else {
        out.push_str(cell);
    }
}
