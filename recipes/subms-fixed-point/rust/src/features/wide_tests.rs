use super::*;

fn f(s: i64, nanos: u32) -> Fixed {
    Fixed::from_parts(s, nanos).unwrap()
}

#[test]
fn widening_and_product_are_exact() {
    let x = f(100, 250_000_000);
    assert_eq!(
        Fixed128::from_fixed(x).raw(),
        100_250_000_000 * SCALE as i128
    );
    let p = Fixed128::product(f(100, 250_000_000), Fixed::from_int(300).unwrap());
    assert_eq!(p.raw(), 30_075 * WIDE_SCALE);
    // 1.000000001 squared keeps all eighteen places
    let tiny = Fixed::from_raw(SCALE + 1);
    assert_eq!(
        Fixed128::product(tiny, tiny).raw(),
        WIDE_SCALE + 2 * SCALE as i128 + 1
    );
    assert_eq!(WIDE_DECIMALS, 18);
    assert_eq!(Fixed128::from_raw(5).raw(), 5);
    assert_eq!(Fixed128::default(), Fixed128::ZERO);
}

#[test]
fn accumulating_products_survives_what_overflows_i64() {
    // 4e9 * 4e9 = 1.6e19 whole units: past i64 at nine decimals.
    let big = Fixed::from_int(4_000_000_000).unwrap();
    assert!(big.checked_mul(big, Rounding::HalfEven).is_none());
    let mut acc = Fixed128::ZERO;
    for _ in 0..10 {
        acc = acc.checked_add_product(big, big).unwrap();
    }
    assert_eq!(acc.raw(), 16_000_000_000_000_000_000 * WIDE_SCALE * 10);
    assert_eq!(acc.to_fixed(Rounding::HalfEven), Err(FixedError::Overflow));
    let avg = acc.checked_div_int(10, Rounding::HalfEven).unwrap();
    assert_eq!(avg.to_fixed(Rounding::HalfEven), Err(FixedError::Overflow));
}

#[test]
fn narrowing_rounds_the_dropped_nine_digits_once() {
    let half_nano = Fixed128::from_raw(500_000_000);
    assert_eq!(half_nano.to_fixed(Rounding::HalfEven), Ok(Fixed::ZERO));
    assert_eq!(half_nano.to_fixed(Rounding::HalfUp), Ok(Fixed::from_raw(1)));
    assert_eq!(
        half_nano.to_fixed(Rounding::Ceiling),
        Ok(Fixed::from_raw(1))
    );
    assert_eq!(
        Fixed128::from_raw(-500_000_000).to_fixed(Rounding::Floor),
        Ok(Fixed::from_raw(-1))
    );
    assert_eq!(
        Fixed128::from_raw(1_500_000_000).to_fixed(Rounding::HalfEven),
        Ok(Fixed::from_raw(2))
    );
    let x = f(123, 456_789_123);
    assert_eq!(Fixed128::from_fixed(x).to_fixed(Rounding::Down), Ok(x));
}

#[test]
fn checked_arithmetic_at_the_i128_edge() {
    let top = Fixed128::from_raw(i128::MAX);
    assert!(top.checked_add(Fixed128::from_raw(1)).is_none());
    assert!(top.checked_add_fixed(Fixed::from_raw(1)).is_none());
    assert!(
        Fixed128::from_raw(i128::MIN)
            .checked_sub(Fixed128::from_raw(1))
            .is_none()
    );
    assert!(Fixed128::from_raw(i128::MIN).checked_neg().is_none());
    assert_eq!(
        Fixed128::from_raw(-7).checked_neg(),
        Some(Fixed128::from_raw(7))
    );
    assert_eq!(
        Fixed128::ZERO.checked_add_fixed(f(1, 0)),
        Some(Fixed128::from_fixed(f(1, 0)))
    );
    assert_eq!(
        Fixed128::from_fixed(f(3, 0)).checked_sub(Fixed128::from_fixed(f(1, 0))),
        Some(Fixed128::from_fixed(f(2, 0)))
    );
    assert!(
        Fixed128::ZERO
            .checked_div_int(0, Rounding::HalfEven)
            .is_none()
    );
}

#[test]
fn division_by_count_rounds_at_eighteen_places() {
    let one = Fixed128::from_fixed(Fixed::ONE);
    let third = one.checked_div_int(3, Rounding::HalfEven).unwrap();
    assert_eq!(third.raw(), 333_333_333_333_333_333);
    assert_eq!(
        one.checked_div_int(3, Rounding::Ceiling).unwrap().raw(),
        333_333_333_333_333_334
    );
    assert_eq!(
        third.to_fixed(Rounding::HalfEven),
        Ok(Fixed::from_raw(333_333_333))
    );
}

#[test]
fn predicates_sum_and_display() {
    assert!(Fixed128::ZERO.is_zero());
    assert!(Fixed128::from_raw(-1).is_negative());
    assert_eq!(Fixed128::from_raw(-1).signum(), -1);
    assert_eq!(Fixed128::from_raw(9).signum(), 1);
    let total: Fixed128 = (1..=4)
        .map(|k| Fixed128::from_fixed(Fixed::from_int(k).unwrap()))
        .sum();
    assert_eq!(total, Fixed128::from_fixed(Fixed::from_int(10).unwrap()));
    assert_eq!(
        Fixed128::from_fixed(f(100, 500_000_000)).to_string(),
        "100.5"
    );
    assert_eq!(Fixed128::from_raw(1).to_string(), "0.000000000000000001");
    assert_eq!(Fixed128::from_raw(-1).to_string(), "-0.000000000000000001");
    assert_eq!(Fixed128::ZERO.to_string(), "0");
    assert_eq!(
        format!("{:?}", Fixed128::from_fixed(Fixed::ONE)),
        "Fixed128(1)"
    );
    assert!(Fixed128::from_raw(1) < Fixed128::from_raw(2));
}

#[test]
#[should_panic(expected = "fixed-point overflow in Fixed128 sum")]
fn sum_panics_on_overflow() {
    let _: Fixed128 = [Fixed128::from_raw(i128::MAX), Fixed128::from_raw(1)]
        .into_iter()
        .sum();
}

#[test]
fn stress_exact_accumulation_against_an_integer_oracle() {
    let mut s = 0xDEAD_BEEF_CAFE_F00Du64;
    let mut acc = Fixed128::ZERO;
    let mut oracle: i128 = 0;
    for _ in 0..200_000 {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let px = Fixed::from_raw((s % 200_000_000_000) as i64);
        let qty = Fixed::from_int(((s >> 40) % 10_000) as i64).unwrap();
        acc = acc.checked_add_product(px, qty).unwrap();
        oracle += px.raw() as i128 * qty.raw() as i128;
    }
    assert_eq!(acc.raw(), oracle);
    let avg = acc.checked_div_int(200_000, Rounding::Floor).unwrap();
    assert_eq!(
        avg.to_fixed(Rounding::Floor).unwrap().raw() as i128,
        oracle.div_euclid(200_000).div_euclid(SCALE as i128)
    );
}
