//! Minimal cron-expression parser + recurring scheduler.
//!
//! Accepts the classic 5-field syntax:
//!
//! ```text
//!   minute  hour  day-of-month  month  day-of-week
//!   0-59    0-23  1-31          1-12   0-6 (Sunday=0)
//! ```
//!
//! Per field we support:
//!   `*`      - every value in the field's range
//!   `*/N`    - every Nth value (step from the field's minimum)
//!   `a-b`    - inclusive range
//!   `a,b,c`  - explicit list (entries may be ranges or steps)
//!   `a`      - single literal value
//!
//! Not supported (out of scope for the minimum-viable recipe): the
//! `L`/`W`/`?` extensions, named months/days, seconds field, and
//! step modifiers attached to ranges (`1-10/2`). If a workload needs
//! those, reach for a full-featured cron crate.
//!
//! `CronSchedule::next_after(epoch_seconds)` returns the next firing
//! second on or after the input. `CronScheduler` ties a `CronSchedule`
//! to a base `TimerWheel`, re-arming the next deadline each time the
//! current one fires.

use std::fmt;

#[derive(Debug, PartialEq, Eq)]
pub enum CronError {
    /// Wrong number of whitespace-separated fields.
    WrongFieldCount(usize),
    /// A field couldn't be parsed (invalid number, out of range, etc.).
    InvalidField(&'static str, String),
}

impl fmt::Display for CronError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CronError::WrongFieldCount(n) => {
                write!(f, "cron expression must have 5 fields, got {n}")
            }
            CronError::InvalidField(name, raw) => write!(f, "invalid {name} field: {raw}"),
        }
    }
}

impl std::error::Error for CronError {}

#[derive(Debug, Clone)]
pub struct CronSchedule {
    minute: Vec<u8>,
    hour: Vec<u8>,
    dom: Vec<u8>,
    month: Vec<u8>,
    dow: Vec<u8>,
}

impl CronSchedule {
    /// Parse a five-field cron expression. Whitespace-separated.
    pub fn parse(expr: &str) -> Result<Self, CronError> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(CronError::WrongFieldCount(fields.len()));
        }
        Ok(Self {
            minute: parse_field(fields[0], 0, 59, "minute")?,
            hour: parse_field(fields[1], 0, 23, "hour")?,
            dom: parse_field(fields[2], 1, 31, "day-of-month")?,
            month: parse_field(fields[3], 1, 12, "month")?,
            dow: parse_field(fields[4], 0, 6, "day-of-week")?,
        })
    }

    pub fn minutes(&self) -> &[u8] {
        &self.minute
    }
    pub fn hours(&self) -> &[u8] {
        &self.hour
    }
    pub fn days_of_month(&self) -> &[u8] {
        &self.dom
    }
    pub fn months(&self) -> &[u8] {
        &self.month
    }
    pub fn days_of_week(&self) -> &[u8] {
        &self.dow
    }

    /// Smallest epoch-second `>= after_epoch` whose minute/hour/dom/
    /// month/dow all match. Returns `None` if no firing exists within
    /// the next ~5 years (defensive cap; real schedules fire within
    /// a year unless misconfigured).
    pub fn next_after(&self, after_epoch: u64) -> Option<u64> {
        // Round up to the next whole minute (cron has minute resolution).
        let mut t = after_epoch.div_ceil(60) * 60;
        let cap = after_epoch + 5 * 365 * 24 * 60 * 60;
        while t < cap {
            let (year, month, dom, dow, hour, minute) = civil_from_epoch(t);
            if !self.minute.contains(&(minute as u8)) {
                t += 60;
                continue;
            }
            if !self.hour.contains(&(hour as u8)) {
                t += 60;
                continue;
            }
            if !self.month.contains(&(month as u8)) {
                t += 60;
                continue;
            }
            // dom + dow: the cron historical convention is OR when
            // both are restrictive, AND when one is `*`. Our parser
            // doesn't track "field was `*`", so we mimic the
            // simple-AND rule, which is correct for the vast majority
            // of recipes (`0 */5 * * *` etc).
            if !self.dom.contains(&(dom as u8)) {
                t += 60;
                continue;
            }
            if !self.dow.contains(&(dow as u8)) {
                t += 60;
                continue;
            }
            let _ = year;
            return Some(t);
        }
        None
    }
}

fn parse_field(s: &str, lo: u32, hi: u32, name: &'static str) -> Result<Vec<u8>, CronError> {
    let mut out = Vec::new();
    for part in s.split(',') {
        let part = part.trim();
        if part.is_empty() {
            return Err(CronError::InvalidField(name, s.to_string()));
        }
        // `*/N`
        if let Some(rest) = part.strip_prefix("*/") {
            let step: u32 = rest
                .parse()
                .map_err(|_| CronError::InvalidField(name, s.to_string()))?;
            if step == 0 {
                return Err(CronError::InvalidField(name, s.to_string()));
            }
            let mut v = lo;
            while v <= hi {
                out.push(v as u8);
                v += step;
            }
            continue;
        }
        // `*`
        if part == "*" {
            for v in lo..=hi {
                out.push(v as u8);
            }
            continue;
        }
        // `a-b`
        if let Some((a, b)) = part.split_once('-') {
            let a: u32 = a
                .parse()
                .map_err(|_| CronError::InvalidField(name, s.to_string()))?;
            let b: u32 = b
                .parse()
                .map_err(|_| CronError::InvalidField(name, s.to_string()))?;
            if a < lo || b > hi || a > b {
                return Err(CronError::InvalidField(name, s.to_string()));
            }
            for v in a..=b {
                out.push(v as u8);
            }
            continue;
        }
        // literal
        let v: u32 = part
            .parse()
            .map_err(|_| CronError::InvalidField(name, s.to_string()))?;
        if v < lo || v > hi {
            return Err(CronError::InvalidField(name, s.to_string()));
        }
        out.push(v as u8);
    }
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

/// Convert epoch-seconds (Unix epoch, UTC) to (year, month, dom, dow, hour, minute).
/// Howard Hinnant's algorithm, adapted to u64. Pre-1970 inputs are clamped to 1970-01-01.
fn civil_from_epoch(epoch: u64) -> (i32, u32, u32, u32, u32, u32) {
    let days_since_epoch = (epoch / 86_400) as i64;
    let secs_today = (epoch % 86_400) as u32;
    let hour = secs_today / 3600;
    let minute = (secs_today % 3600) / 60;
    // dow: 1970-01-01 was Thursday = 4. Sunday = 0.
    let dow = (((days_since_epoch + 4) % 7 + 7) % 7) as u32;

    // Howard Hinnant's civil_from_days.
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 {
        z / 146_097
    } else {
        (z - 146_096) / 146_097
    };
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dom = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = (y + if month <= 2 { 1 } else { 0 }) as i32;
    (year, month, dom, dow, hour, minute)
}

/// Recurring scheduler glued to a base `TimerWheel`. After each fire
/// the scheduler computes the next deadline from the cron schedule
/// and re-arms.
pub struct CronScheduler {
    schedule: CronSchedule,
    last_fire_epoch: u64,
}

impl CronScheduler {
    pub fn new(schedule: CronSchedule, now_epoch: u64) -> Self {
        Self {
            schedule,
            last_fire_epoch: now_epoch,
        }
    }

    pub fn schedule(&self) -> &CronSchedule {
        &self.schedule
    }

    /// Epoch-second the schedule will next fire, given the current
    /// epoch second. Returns `None` if no firing within the schedule's
    /// look-ahead horizon.
    pub fn next_fire(&self, now_epoch: u64) -> Option<u64> {
        let after = now_epoch.max(self.last_fire_epoch + 1);
        self.schedule.next_after(after)
    }

    /// Mark a fire at `epoch` consumed; next call to `next_fire` will
    /// look past it.
    pub fn record_fire(&mut self, epoch: u64) {
        self.last_fire_epoch = epoch;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_star_field_expands_to_full_range() {
        let s = CronSchedule::parse("* * * * *").unwrap();
        assert_eq!(s.minutes().len(), 60);
        assert_eq!(s.hours().len(), 24);
        assert_eq!(s.days_of_month().len(), 31);
        assert_eq!(s.months().len(), 12);
        assert_eq!(s.days_of_week().len(), 7);
    }

    #[test]
    fn parse_step_expression_picks_correct_minutes() {
        let s = CronSchedule::parse("*/15 * * * *").unwrap();
        assert_eq!(s.minutes(), &[0, 15, 30, 45]);
    }

    #[test]
    fn parse_list_and_range_combine() {
        let s = CronSchedule::parse("0,30 1-3 * * *").unwrap();
        assert_eq!(s.minutes(), &[0, 30]);
        assert_eq!(s.hours(), &[1, 2, 3]);
    }

    #[test]
    fn parse_literal_value() {
        let s = CronSchedule::parse("15 14 1 1 *").unwrap();
        assert_eq!(s.minutes(), &[15]);
        assert_eq!(s.hours(), &[14]);
        assert_eq!(s.days_of_month(), &[1]);
        assert_eq!(s.months(), &[1]);
    }

    #[test]
    fn parse_rejects_wrong_field_count() {
        match CronSchedule::parse("* * * *") {
            Err(CronError::WrongFieldCount(4)) => {}
            other => panic!("expected WrongFieldCount(4): {other:?}"),
        }
        match CronSchedule::parse("* * * * * *") {
            Err(CronError::WrongFieldCount(6)) => {}
            other => panic!("expected WrongFieldCount(6): {other:?}"),
        }
    }

    #[test]
    fn parse_rejects_out_of_range_minute() {
        let err = CronSchedule::parse("60 * * * *").unwrap_err();
        match err {
            CronError::InvalidField("minute", _) => {}
            _ => panic!("expected InvalidField(minute): {err:?}"),
        }
    }

    #[test]
    fn parse_rejects_inverted_range() {
        let err = CronSchedule::parse("5-1 * * * *").unwrap_err();
        match err {
            CronError::InvalidField("minute", _) => {}
            _ => panic!("expected InvalidField(minute): {err:?}"),
        }
    }

    #[test]
    fn parse_rejects_zero_step() {
        let err = CronSchedule::parse("*/0 * * * *").unwrap_err();
        assert!(matches!(err, CronError::InvalidField("minute", _)));
    }

    #[test]
    fn parse_rejects_non_numeric_field() {
        let err = CronSchedule::parse("abc * * * *").unwrap_err();
        assert!(matches!(err, CronError::InvalidField("minute", _)));
    }

    #[test]
    fn parse_rejects_empty_list_entry() {
        let err = CronSchedule::parse("1,,2 * * * *").unwrap_err();
        assert!(matches!(err, CronError::InvalidField("minute", _)));
    }

    #[test]
    fn civil_from_epoch_returns_known_anchor() {
        // 2024-01-01 00:00:00 UTC = epoch 1_704_067_200
        let (y, m, d, dow, h, mn) = civil_from_epoch(1_704_067_200);
        assert_eq!((y, m, d, h, mn), (2024, 1, 1, 0, 0));
        // Monday = 1.
        assert_eq!(dow, 1);
    }

    #[test]
    fn next_after_for_every_minute() {
        let s = CronSchedule::parse("* * * * *").unwrap();
        // After 2024-01-01 00:00:30 UTC, next fire is 00:01:00 = +30s.
        let now = 1_704_067_230;
        assert_eq!(s.next_after(now), Some(1_704_067_260));
    }

    #[test]
    fn next_after_for_every_five_minutes() {
        let s = CronSchedule::parse("*/5 * * * *").unwrap();
        // After 2024-01-01 00:00:00 (already aligned). Next fire is
        // 00:05:00 = +300s when input is after_epoch=00:00:01.
        let now = 1_704_067_201;
        assert_eq!(s.next_after(now), Some(1_704_067_500));
    }

    #[test]
    fn next_after_respects_hour_filter() {
        let s = CronSchedule::parse("0 14 * * *").unwrap();
        // 2024-01-01 13:00:00 UTC = anchor + 13*3600.
        let now = 1_704_067_200 + 13 * 3600;
        // Next 14:00 UTC = +1h = anchor + 14*3600 = 1_704_117_600.
        assert_eq!(s.next_after(now), Some(1_704_067_200 + 14 * 3600));
    }

    #[test]
    fn cron_scheduler_advances_past_recorded_fire() {
        let s = CronSchedule::parse("* * * * *").unwrap();
        let mut cs = CronScheduler::new(s, 1_704_067_200);
        let first = cs.next_fire(1_704_067_200).unwrap();
        assert_eq!(first, 1_704_067_260);
        cs.record_fire(first);
        let second = cs.next_fire(first).unwrap();
        assert_eq!(second, first + 60);
    }

    #[test]
    fn display_error_messages_are_descriptive() {
        let e = CronError::WrongFieldCount(3);
        assert!(e.to_string().contains("5 fields"));
        let e = CronError::InvalidField("minute", "60".to_string());
        assert!(e.to_string().contains("minute"));
    }
}
