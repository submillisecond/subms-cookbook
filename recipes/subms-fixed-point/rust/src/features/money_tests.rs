use super::*;

fn usd(s: &str) -> Money {
    Money::new(parse(s), Currency::USD)
}

fn parse(s: &str) -> Fixed {
    let (neg, s) = match s.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, s),
    };
    let (i, f) = s.split_once('.').unwrap_or((s, ""));
    let mut nanos = f.to_string();
    while nanos.len() < 9 {
        nanos.push('0');
    }
    let v = Fixed::from_parts(i.parse().unwrap(), nanos.parse().unwrap()).unwrap();
    if neg { -v } else { v }
}

#[test]
fn currency_codes_validate_normalise_and_round_trip() {
    assert_eq!(Currency::new("usd").unwrap(), Currency::USD);
    assert_eq!(Currency::new("Eur").unwrap(), Currency::EUR);
    assert_eq!(Currency::USD.code(), *b"USD");
    assert_eq!(Currency::USD.to_string(), "USD");
    assert_eq!(format!("{:?}", Currency::GBP), "Currency(GBP)");
    for bad in ["US", "USDD", "U$D", "", "123"] {
        assert_eq!(
            Currency::new(bad),
            Err(FixedError::InvalidCurrency),
            "{bad:?}"
        );
    }
    assert!(Currency::USD < Currency::new("XAU").unwrap());
}

#[test]
fn minor_unit_exponents_follow_iso_4217() {
    assert_eq!(Currency::USD.minor_unit_decimals(), 2);
    assert_eq!(Currency::EUR.minor_unit_decimals(), 2);
    assert_eq!(Currency::JPY.minor_unit_decimals(), 0);
    assert_eq!(Currency::KRW.minor_unit_decimals(), 0);
    assert_eq!(Currency::BHD.minor_unit_decimals(), 3);
    assert_eq!(Currency::KWD.minor_unit_decimals(), 3);
    assert_eq!(Currency::new("XYZ").unwrap().minor_unit_decimals(), 2);
}

#[test]
fn same_currency_arithmetic_is_exact() {
    let a = usd("100.25");
    let b = usd("0.75");
    assert_eq!(a.checked_add(b).unwrap(), usd("101"));
    assert_eq!(a.checked_sub(b).unwrap(), usd("99.5"));
    assert_eq!(a.amount(), parse("100.25"));
    assert_eq!(a.currency(), Currency::USD);
    assert_eq!(Money::zero(Currency::USD).checked_add(a).unwrap(), a);
    assert!(Money::zero(Currency::JPY).is_zero());
    assert!(usd("-1").is_negative());
    assert!(!a.is_negative());
}

#[test]
fn cross_currency_arithmetic_is_refused() {
    let a = usd("100");
    let e = Money::new(parse("100"), Currency::EUR);
    assert_eq!(a.checked_add(e), Err(FixedError::CurrencyMismatch));
    assert_eq!(a.checked_sub(e), Err(FixedError::CurrencyMismatch));
    assert_ne!(a, e);
}

#[test]
fn overflow_is_reported() {
    let big = Money::new(Fixed::MAX, Currency::USD);
    assert_eq!(big.checked_add(usd("1")), Err(FixedError::Overflow));
    let small = Money::new(Fixed::MIN, Currency::USD);
    assert_eq!(small.checked_sub(usd("1")), Err(FixedError::Overflow));
    assert_eq!(
        big.checked_mul(parse("2"), Rounding::HalfEven),
        Err(FixedError::Overflow)
    );
    assert_eq!(
        big.convert(parse("2"), Currency::EUR, Rounding::HalfEven),
        Err(FixedError::Overflow)
    );
}

#[test]
fn scaling_and_conversion_round_once_under_the_named_mode() {
    let fee = usd("1000")
        .checked_mul(parse("0.000125"), Rounding::HalfEven)
        .unwrap();
    assert_eq!(fee, usd("0.125"));
    let eur = usd("100")
        .convert(parse("0.9215"), Currency::EUR, Rounding::HalfEven)
        .unwrap();
    assert_eq!(eur, Money::new(parse("92.15"), Currency::EUR));
    // 1 / 3 at a rate of 0.333333333333: rounding sits at the ninth place
    let third = usd("1")
        .convert(
            Fixed::from_raw(333_333_333),
            Currency::EUR,
            Rounding::HalfEven,
        )
        .unwrap();
    assert_eq!(third.amount().raw(), 333_333_333);
}

#[test]
fn settlement_rounds_to_the_currency_minor_unit() {
    let x = usd("10.125");
    assert_eq!(x.round_to_minor(Rounding::HalfEven), usd("10.12"));
    assert_eq!(x.round_to_minor(Rounding::HalfUp), usd("10.13"));
    assert!(!x.is_settleable());
    assert!(usd("10.12").is_settleable());
    let y = Money::new(parse("1234.5"), Currency::JPY);
    assert_eq!(y.round_to_minor(Rounding::HalfEven).amount(), parse("1234"));
    assert_eq!(y.round_to_minor(Rounding::HalfUp).amount(), parse("1235"));
    let b = Money::new(parse("1.23456"), Currency::BHD);
    assert_eq!(
        b.round_to_minor(Rounding::HalfEven).amount(),
        parse("1.235")
    );
    assert!(Money::new(parse("1.234"), Currency::BHD).is_settleable());
    let neg = usd("-10.125");
    assert_eq!(neg.round_to_minor(Rounding::Floor), usd("-10.13"));
    assert_eq!(neg.round_to_minor(Rounding::Ceiling), usd("-10.12"));
}

#[test]
fn display_and_hash() {
    use std::collections::HashSet;
    assert_eq!(usd("100.5").to_string(), "100.5 USD");
    assert_eq!(Money::new(parse("-3"), Currency::JPY).to_string(), "-3 JPY");
    let set: HashSet<Money> = [usd("1"), usd("1"), usd("2")].into_iter().collect();
    assert_eq!(set.len(), 2);
}
