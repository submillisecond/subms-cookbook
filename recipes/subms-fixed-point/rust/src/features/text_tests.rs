use super::*;

#[test]
fn parses_the_common_shapes() {
    assert_eq!(Fixed::parse("100.25").unwrap().raw(), 100_250_000_000);
    assert_eq!(Fixed::parse("-100.25").unwrap().raw(), -100_250_000_000);
    assert_eq!(Fixed::parse("+7").unwrap().raw(), 7 * SCALE);
    assert_eq!(Fixed::parse("0100.250").unwrap().raw(), 100_250_000_000);
    assert_eq!(Fixed::parse(".5").unwrap().raw(), 500_000_000);
    assert_eq!(Fixed::parse("5.").unwrap().raw(), 5 * SCALE);
    assert_eq!(Fixed::parse("0.000000001").unwrap().raw(), 1);
    assert_eq!(Fixed::parse("-0.000000001").unwrap().raw(), -1);
    assert_eq!(Fixed::parse("0").unwrap(), Fixed::ZERO);
    assert_eq!(Fixed::parse("-0").unwrap(), Fixed::ZERO);
    assert_eq!("12.5".parse::<Fixed>().unwrap().raw(), 12_500_000_000);
}

#[test]
fn rejects_what_is_not_a_plain_decimal() {
    for bad in [
        "", "-", "+", ".", "1e3", " 1", "1 ", "1,000", "1.2.3", "abc", "--1", "0x10",
    ] {
        assert_eq!(Fixed::parse(bad), Err(FixedError::InvalidText), "{bad:?}");
    }
}

#[test]
fn ten_fractional_digits_is_too_precise_even_when_zero() {
    assert_eq!(Fixed::parse("1.0000000000"), Err(FixedError::TooPrecise));
    assert_eq!(Fixed::parse("1.0000000001"), Err(FixedError::TooPrecise));
    assert!(Fixed::parse("1.000000000").is_ok());
}

#[test]
fn integer_overflow_is_reported_not_wrapped() {
    assert_eq!(Fixed::parse("9223372036.854775807").unwrap(), Fixed::MAX);
    assert_eq!(
        Fixed::parse("9223372036.854775808"),
        Err(FixedError::Overflow)
    );
    assert_eq!(Fixed::parse("9223372037"), Err(FixedError::Overflow));
    assert_eq!(
        Fixed::parse("99999999999999999999"),
        Err(FixedError::Overflow)
    );
    // MIN itself is not reachable through the positive-magnitude path.
    assert_eq!(
        Fixed::parse("-9223372036.854775807").unwrap().raw(),
        -i64::MAX
    );
}

#[test]
fn write_to_fixes_the_decimal_count_and_truncates() {
    let x = Fixed::from_parts(100, 256_789_000).unwrap();
    let mut buf = [0u8; 32];
    let n = x.write_to(&mut buf, 2).unwrap();
    assert_eq!(&buf[..n], b"100.25");
    let n = x.write_to(&mut buf, 0).unwrap();
    assert_eq!(&buf[..n], b"100");
    let n = x.write_to(&mut buf, 9).unwrap();
    assert_eq!(&buf[..n], b"100.256789000");
    let n = Fixed::from_raw(-1).write_to(&mut buf, 9).unwrap();
    assert_eq!(&buf[..n], b"-0.000000001");
    let n = Fixed::from_raw(-1).write_to(&mut buf, 2).unwrap();
    assert_eq!(&buf[..n], b"-0.00");
    let n = Fixed::ZERO.write_to(&mut buf, 4).unwrap();
    assert_eq!(&buf[..n], b"0.0000");
    let n = Fixed::MIN.write_to(&mut buf, 9).unwrap();
    assert_eq!(&buf[..n], b"-9223372036.854775808");
    assert!(n <= MAX_TEXT_LEN);
}

#[test]
fn write_to_reports_a_short_buffer_and_too_many_decimals() {
    let x = Fixed::from_parts(100, 250_000_000).unwrap();
    let mut small = [0u8; 5];
    assert_eq!(x.write_to(&mut small, 2), Err(FixedError::BufferTooSmall));
    let mut buf = [0u8; 32];
    assert_eq!(x.write_to(&mut buf, 10), Err(FixedError::TooPrecise));
}

#[test]
fn string_convenience_matches_the_buffer_form() {
    let x = Fixed::from_parts(-3, 5_000_000).unwrap();
    assert_eq!(x.to_string_decimals(3).unwrap(), "-3.005");
    assert_eq!(x.to_string_decimals(0).unwrap(), "-3");
    assert_eq!(x.to_string_decimals(12), Err(FixedError::TooPrecise));
}

#[test]
fn round_trip_is_identity_on_random_values() {
    let mut s = 0x9E37_79B9_7F4A_7C15u64;
    let mut buf = [0u8; 32];
    for _ in 0..100_000 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let x = Fixed::from_raw(s as i64 / 3);
        let n = x.write_to(&mut buf, 9).unwrap();
        assert_eq!(Fixed::parse_bytes(&buf[..n]).unwrap(), x);
        assert_eq!(Fixed::parse(&x.to_string()).unwrap(), x);
    }
}
