//! Hand-rolled NDJSON ingest: one flat JSON object per line. The reader takes
//! the union of object keys (in first-seen order) as the column set; a key
//! absent on a line is a gap for that column at that row. Values are scanned
//! as raw text tokens and handed to the same per-column inference as CSV, so a
//! `{"n": 1}` line and a `{"n": "1"}` line are distinguished only by the quote
//! - a quoted value is forced to text, an unquoted one is inferred.
//!
//! This is deliberately a FLAT-object parser. A value that is itself an object
//! or an array is rejected (`BadJson`) rather than being flattened into
//! synthetic columns - nested-JSON-to-columns is an explicit non-claim.

use subms_ts::TsDataFrame;

use crate::{RawColumn, TsCsvError, TsCsvOptions, assemble};

/// A scanned field value plus whether it arrived quoted (forces `Str`).
struct Scanned {
    raw: String,
    quoted: bool,
}

/// Parse `text` as NDJSON into a [`TsDataFrame`]. Blank lines are skipped. The
/// ts axis is `opts.ts_column` (its value parsed as an `i64`) or the row index.
pub fn read_ndjson(text: &str, opts: &TsCsvOptions) -> Result<TsDataFrame, TsCsvError> {
    let mut names: Vec<String> = Vec::new();
    // raw cells per column, indexed parallel to `names`. A column discovered
    // late simply has no cells for the earlier rows (those rows are gaps).
    let mut cells: Vec<Vec<(i64, String)>> = Vec::new();
    // columns whose value ever arrived quoted are pinned to Str.
    let mut quoted: Vec<bool> = Vec::new();

    let mut row: i64 = 0;
    for (line_no, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let fields = parse_object(line, line_no)?;

        let ts = match &opts.ts_column {
            Some(name) => {
                let f = fields
                    .iter()
                    .find(|(k, _)| k == name)
                    .ok_or_else(|| TsCsvError::BadTimestamp {
                        row: line_no,
                        value: format!("missing key {name}"),
                    })?;
                f.1.raw
                    .trim()
                    .parse::<i64>()
                    .map_err(|_| TsCsvError::BadTimestamp {
                        row: line_no,
                        value: f.1.raw.clone(),
                    })?
            }
            None => row,
        };

        for (key, scanned) in fields {
            if opts.ts_column.as_deref() == Some(key.as_str()) {
                continue;
            }
            // a JSON null is a gap, same as a CSV empty cell.
            if scanned.raw == "\0null" {
                continue;
            }
            let idx = match names.iter().position(|n| n == &key) {
                Some(i) => i,
                None => {
                    names.push(key);
                    cells.push(Vec::new());
                    quoted.push(false);
                    names.len() - 1
                }
            };
            if scanned.quoted {
                quoted[idx] = true;
            }
            cells[idx].push((ts, scanned.raw));
        }
        row += 1;
    }

    let raws: Vec<RawColumn> = cells
        .into_iter()
        .zip(quoted)
        .map(|(col_cells, is_quoted)| RawColumn {
            cells: col_cells,
            forced_str: is_quoted,
        })
        .collect();

    Ok(assemble(names, raws))
}

/// Parse one flat JSON object line into `(key, Scanned)` pairs in order.
/// Rejects a non-object top level, a nested object / array value, a trailing
/// comma, and structurally malformed input.
fn parse_object(line: &str, line_no: usize) -> Result<Vec<(String, Scanned)>, TsCsvError> {
    let bytes: Vec<char> = line.chars().collect();
    let mut i = 0usize;
    let bad = |hint: &'static str| TsCsvError::BadJson { line: line_no, hint };

    skip_ws(&bytes, &mut i);
    if bytes.get(i) != Some(&'{') {
        return Err(bad("expected object"));
    }
    i += 1;
    skip_ws(&bytes, &mut i);

    let mut out: Vec<(String, Scanned)> = Vec::new();
    if bytes.get(i) == Some(&'}') {
        return Ok(out);
    }

    loop {
        skip_ws(&bytes, &mut i);
        if bytes.get(i) != Some(&'"') {
            return Err(bad("expected string key"));
        }
        let key = parse_string(&bytes, &mut i).ok_or_else(|| bad("bad key string"))?;
        skip_ws(&bytes, &mut i);
        if bytes.get(i) != Some(&':') {
            return Err(bad("expected colon"));
        }
        i += 1;
        skip_ws(&bytes, &mut i);
        let value = parse_value(&bytes, &mut i, line_no)?;
        out.push((key, value));
        skip_ws(&bytes, &mut i);
        match bytes.get(i) {
            Some(&',') => {
                i += 1;
                continue;
            }
            Some(&'}') => {
                i += 1;
                break;
            }
            _ => return Err(bad("expected comma or close brace")),
        }
    }

    skip_ws(&bytes, &mut i);
    if i != bytes.len() {
        return Err(bad("trailing content after object"));
    }
    Ok(out)
}

fn skip_ws(b: &[char], i: &mut usize) {
    while let Some(c) = b.get(*i) {
        if c.is_whitespace() {
            *i += 1;
        } else {
            break;
        }
    }
}

/// Parse a `"..."` JSON string starting at `b[*i] == '"'`. Honours the JSON
/// escapes the flat-value contract needs: `\" \\ \/ \n \r \t \b \f` and
/// `\uXXXX`. Returns `None` on an unterminated or malformed escape.
fn parse_string(b: &[char], i: &mut usize) -> Option<String> {
    if b.get(*i) != Some(&'"') {
        return None;
    }
    *i += 1;
    let mut s = String::new();
    while let Some(&c) = b.get(*i) {
        *i += 1;
        match c {
            '"' => return Some(s),
            '\\' => {
                let esc = *b.get(*i)?;
                *i += 1;
                match esc {
                    '"' => s.push('"'),
                    '\\' => s.push('\\'),
                    '/' => s.push('/'),
                    'n' => s.push('\n'),
                    'r' => s.push('\r'),
                    't' => s.push('\t'),
                    'b' => s.push('\u{0008}'),
                    'f' => s.push('\u{000C}'),
                    'u' => {
                        let mut code: u32 = 0;
                        for _ in 0..4 {
                            let h = *b.get(*i)?;
                            *i += 1;
                            code = code * 16 + h.to_digit(16)?;
                        }
                        s.push(char::from_u32(code)?);
                    }
                    _ => return None,
                }
            }
            other => s.push(other),
        }
    }
    None
}

/// Parse a flat JSON value: string, number, bool, or null. An object `{` or
/// array `[` is a hard error - nested JSON is not flattened into columns.
fn parse_value(b: &[char], i: &mut usize, line_no: usize) -> Result<Scanned, TsCsvError> {
    let bad = |hint: &'static str| TsCsvError::BadJson { line: line_no, hint };
    match b.get(*i) {
        Some(&'"') => {
            let s = parse_string(b, i).ok_or_else(|| bad("bad string value"))?;
            Ok(Scanned {
                raw: s,
                quoted: true,
            })
        }
        Some(&'{') | Some(&'[') => Err(bad("nested object/array not supported")),
        Some(_) => {
            // a bare token: number, true, false, or null. Read up to the next
            // structural char or whitespace.
            let start = *i;
            while let Some(&c) = b.get(*i) {
                if c == ',' || c == '}' || c == ']' || c.is_whitespace() {
                    break;
                }
                *i += 1;
            }
            let token: String = b[start..*i].iter().collect();
            if token.is_empty() {
                return Err(bad("empty value"));
            }
            if token == "null" {
                // tag a null so the caller can drop it as a gap.
                Ok(Scanned {
                    raw: "\0null".to_string(),
                    quoted: false,
                })
            } else {
                Ok(Scanned {
                    raw: token,
                    quoted: false,
                })
            }
        }
        None => Err(bad("missing value")),
    }
}
