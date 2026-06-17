//! RFC3339 (UTC) to/from epoch-nanoseconds, zero-dep.
//!
//! InfluxDB writes accept integer nanosecond timestamps directly, but a Flux
//! query response carries `_time` as an RFC3339 string. We only ever speak
//! UTC with a trailing `Z`, optionally with a fractional-second part, which is
//! exactly what Influx emits - so the parser is a fixed-shape scan, not a
//! general calendar library.

const DAYS_IN_MONTH: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    // Days since 1970-01-01. Howard Hinnant's civil-from-days, inverted.
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Parse an RFC3339 UTC timestamp (`2026-05-31T14:00:00Z`, optional
/// `.fffffffff` fraction) into epoch nanoseconds. Returns `None` on any
/// shape we do not recognise.
pub fn parse_rfc3339_nanos(s: &str) -> Option<i64> {
    let b = s.as_bytes();
    if b.len() < 20 {
        return None;
    }
    let num = |lo: usize, hi: usize| -> Option<i64> {
        let mut v: i64 = 0;
        for &c in &b[lo..hi] {
            if !c.is_ascii_digit() {
                return None;
            }
            v = v * 10 + (c - b'0') as i64;
        }
        Some(v)
    };
    if b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' {
        return None;
    }
    let year = num(0, 4)?;
    let month = num(5, 7)?;
    let day = num(8, 10)?;
    let hour = num(11, 13)?;
    let min = num(14, 16)?;
    let sec = num(17, 19)?;
    if !(1..=12).contains(&month) || day < 1 || hour > 23 || min > 59 || sec > 60 {
        return None;
    }
    let max_day =
        DAYS_IN_MONTH[(month - 1) as usize] + if month == 2 && is_leap(year) { 1 } else { 0 };
    if day > max_day {
        return None;
    }

    let mut idx = 19;
    let mut frac_nanos: i64 = 0;
    if idx < b.len() && b[idx] == b'.' {
        idx += 1;
        let start = idx;
        while idx < b.len() && b[idx].is_ascii_digit() {
            idx += 1;
        }
        let digits = &s[start..idx];
        if digits.is_empty() {
            return None;
        }
        // Right-pad/truncate to 9 digits (nanosecond resolution).
        let mut scaled = String::with_capacity(9);
        for i in 0..9 {
            scaled.push(digits.as_bytes().get(i).copied().unwrap_or(b'0') as char);
        }
        frac_nanos = scaled.parse().ok()?;
    }
    if idx >= b.len() || b[idx] != b'Z' || idx + 1 != b.len() {
        return None;
    }

    let days = days_from_civil(year, month, day);
    let secs = days * 86_400 + hour * 3_600 + min * 60 + sec;
    Some(secs * 1_000_000_000 + frac_nanos)
}

/// Format epoch nanoseconds as an RFC3339 UTC timestamp. Emits a 9-digit
/// fractional part only when the value is not on a whole second.
pub fn format_rfc3339_nanos(nanos: i64) -> String {
    let mut secs = nanos.div_euclid(1_000_000_000);
    let frac = nanos.rem_euclid(1_000_000_000);
    let days = secs.div_euclid(86_400);
    secs = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    let (hh, mm, ss) = (secs / 3_600, (secs % 3_600) / 60, secs % 60);
    if frac == 0 {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
    } else {
        format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}.{frac:09}Z")
    }
}
