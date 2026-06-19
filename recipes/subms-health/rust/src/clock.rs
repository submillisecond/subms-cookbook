//! Clock seam. The registry reads wall-clock millis (for per-component staleness)
//! and an RFC3339 stamp (for the report's `refreshed_at`) through this trait, so
//! tests can freeze time and pin a byte-exact cross-language fixture.

use std::time::{SystemTime, UNIX_EPOCH};

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
    fn now_rfc3339(&self) -> String;
}

/// Real wall clock.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
    fn now_rfc3339(&self) -> String {
        stamp(self.now_ms())
    }
}

/// Fixed clock for tests / deterministic fixtures.
pub struct FixedClock {
    pub ms: u64,
    pub rfc3339: String,
}

impl FixedClock {
    pub fn new(ms: u64, rfc3339: &str) -> Self {
        Self {
            ms,
            rfc3339: rfc3339.to_string(),
        }
    }
}

impl Clock for FixedClock {
    fn now_ms(&self) -> u64 {
        self.ms
    }
    fn now_rfc3339(&self) -> String {
        self.rfc3339.clone()
    }
}

#[cfg(feature = "datetime")]
fn stamp(_ms: u64) -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(not(feature = "datetime"))]
fn stamp(ms: u64) -> String {
    rfc3339_from_secs((ms / 1000) as i64)
}

/// Hand-rolled epoch-seconds -> `YYYY-MM-DDTHH:MM:SSZ`. Same civil-date algorithm
/// the subms harness uses, so timestamps read identically with or without chrono.
#[cfg(not(feature = "datetime"))]
fn rfc3339_from_secs(secs: i64) -> String {
    let mut year = 1970i64;
    let mut days = secs / 86_400;
    let rem = secs % 86_400;
    let hour = rem / 3600;
    let minute = (rem % 3600) / 60;
    let second = rem % 60;
    while days >= year_days(year) {
        days -= year_days(year);
        year += 1;
    }
    let mut month = 1u32;
    for m in 1..=12 {
        let dm = month_days(year, m);
        if days < dm as i64 {
            month = m;
            break;
        }
        days -= dm as i64;
    }
    let day = (days + 1) as u32;
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

#[cfg(not(feature = "datetime"))]
fn year_days(y: i64) -> i64 {
    if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
        366
    } else {
        365
    }
}

#[cfg(not(feature = "datetime"))]
fn month_days(y: i64, m: u32) -> u32 {
    match m {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0) {
                29
            } else {
                28
            }
        }
        _ => 0,
    }
}
