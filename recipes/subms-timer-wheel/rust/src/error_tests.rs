use super::*;

#[test]
fn delay_too_long_displays_both_numbers() {
    let e = TimerError::DelayTooLong { delay: 99, max: 64 };
    let s = e.to_string();
    assert!(s.contains("99"), "{s}");
    assert!(s.contains("64"), "{s}");
}

#[test]
fn implements_std_error() {
    let e = TimerError::DelayTooLong { delay: 1, max: 0 };
    let boxed: Box<dyn std::error::Error> = Box::new(e);
    assert!(boxed.source().is_none());
}
