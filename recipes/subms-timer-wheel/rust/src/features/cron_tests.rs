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
fn scheduler_exposes_its_underlying_schedule() {
    let s = CronSchedule::parse("30 4 * * *").unwrap();
    let cs = CronScheduler::new(s, 1_704_067_200);
    assert_eq!(cs.schedule().minutes(), &[30u8]);
    assert_eq!(cs.schedule().hours(), &[4u8]);
}

#[test]
fn display_error_messages_are_descriptive() {
    let e = CronError::WrongFieldCount(3);
    assert!(e.to_string().contains("5 fields"));
    let e = CronError::InvalidField("minute", "60".to_string());
    assert!(e.to_string().contains("minute"));
}
