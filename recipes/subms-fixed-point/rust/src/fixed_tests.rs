use super::*;

fn f(s: i64, nanos: u32) -> Fixed {
    Fixed::from_parts(s, nanos).unwrap()
}

#[test]
fn constructors_agree_on_the_raw_representation() {
    assert_eq!(Fixed::from_int(1).unwrap(), Fixed::ONE);
    assert_eq!(Fixed::ONE.raw(), SCALE);
    assert_eq!(f(100, 250_000_000).raw(), 100_250_000_000);
    assert_eq!(f(-1, 500_000_000).raw(), -1_500_000_000);
    assert_eq!(Fixed::from_raw(7).raw(), 7);
    assert_eq!(Fixed::default(), Fixed::ZERO);
    assert_eq!(DECIMALS, 9);
}

#[test]
fn constructors_refuse_what_does_not_fit() {
    assert!(Fixed::from_int(i64::MAX).is_none());
    assert!(Fixed::from_int(9_223_372_036).is_some());
    assert!(Fixed::from_int(9_223_372_037).is_none());
    assert!(Fixed::from_parts(0, SCALE as u32).is_none());
    assert!(Fixed::from_parts(9_223_372_036, 854_775_807).is_some());
    assert!(Fixed::from_parts(9_223_372_036, 854_775_808).is_none());
    assert!(Fixed::from_parts(i64::MIN, 1).is_none());
}

#[test]
fn ordering_hashing_and_equality_are_the_integers() {
    use std::collections::HashSet;
    let a = f(100, 0);
    let b = f(100, 1);
    assert!(a < b);
    assert_eq!(a.max(b), b);
    assert_eq!(f(1, 500_000_000), Fixed::from_raw(1_500_000_000));
    let set: HashSet<Fixed> = [a, b, a].into_iter().collect();
    assert_eq!(set.len(), 2);
    assert_eq!(Ord::clamp(f(5, 0), f(1, 0), f(3, 0)), f(3, 0));
}

#[test]
fn addition_and_subtraction_are_exact_and_associative() {
    let tenth = f(0, 100_000_000);
    let mut sum = Fixed::ZERO;
    for _ in 0..10 {
        sum += tenth;
    }
    assert_eq!(sum, Fixed::ONE);
    let forward: Fixed = (1..=1_000).map(Fixed::from_raw).sum();
    let backward: Fixed = (1..=1_000).rev().map(Fixed::from_raw).sum();
    assert_eq!(forward, backward);
    assert_eq!(forward.raw(), 500_500);
    let refs: Fixed = [tenth, tenth].iter().sum();
    assert_eq!(refs, f(0, 200_000_000));
    let mut x = Fixed::ONE;
    x -= tenth;
    assert_eq!(x, f(0, 900_000_000));
    assert_eq!(-x, Fixed::from_raw(-900_000_000));
}

#[test]
fn checked_and_saturating_arithmetic_at_the_edges() {
    assert!(Fixed::MAX.checked_add(Fixed::from_raw(1)).is_none());
    assert!(Fixed::MIN.checked_sub(Fixed::from_raw(1)).is_none());
    assert!(Fixed::MIN.checked_neg().is_none());
    assert!(Fixed::MIN.checked_abs().is_none());
    assert_eq!(Fixed::from_raw(-5).checked_abs(), Some(Fixed::from_raw(5)));
    assert_eq!(Fixed::MAX.saturating_add(Fixed::ONE), Fixed::MAX);
    assert_eq!(Fixed::MIN.saturating_sub(Fixed::ONE), Fixed::MIN);
    assert!(Fixed::MAX.checked_mul_int(2).is_none());
    assert_eq!(f(2, 0).checked_mul_int(3), Some(f(6, 0)));
}

#[test]
#[should_panic(expected = "fixed-point overflow in add")]
fn operator_add_panics_on_overflow() {
    let _ = Fixed::MAX + Fixed::ONE;
}

#[test]
#[should_panic(expected = "fixed-point overflow in sub")]
fn operator_sub_panics_on_overflow() {
    let _ = Fixed::MIN - Fixed::ONE;
}

#[test]
#[should_panic(expected = "fixed-point overflow in neg")]
fn operator_neg_panics_on_min() {
    let _ = -Fixed::MIN;
}

#[test]
fn multiplication_widens_and_rounds_once() {
    let px = f(100, 250_000_000);
    let qty = Fixed::from_int(300).unwrap();
    assert_eq!(
        px.checked_mul(qty, Rounding::HalfEven),
        Fixed::from_int(30_075)
    );
    // 1.000000001 * 1.000000001 = 1.000000002000000001 -> rounds at the 9th place
    let tiny = Fixed::from_raw(SCALE + 1);
    assert_eq!(
        tiny.checked_mul(tiny, Rounding::HalfEven),
        Some(Fixed::from_raw(SCALE + 2))
    );
    assert_eq!(
        tiny.checked_mul(tiny, Rounding::Up),
        Some(Fixed::from_raw(SCALE + 3))
    );
    // 0.5 * 0.000000001 = 0.0000000005, a tie: even goes to 0, half-up to 1
    let half = f(0, 500_000_000);
    let one_nano = Fixed::from_raw(1);
    assert_eq!(
        half.checked_mul(one_nano, Rounding::HalfEven),
        Some(Fixed::ZERO)
    );
    assert_eq!(half.checked_mul(one_nano, Rounding::HalfUp), Some(one_nano));
    assert_eq!(
        half.checked_mul(-one_nano, Rounding::HalfUp),
        Some(-one_nano)
    );
    // The product's magnitude can exceed i64 even when both inputs fit.
    let big = Fixed::from_int(4_000_000_000).unwrap();
    assert!(big.checked_mul(big, Rounding::HalfEven).is_none());
    assert!(
        Fixed::MAX
            .checked_mul(Fixed::MAX, Rounding::HalfEven)
            .is_none()
    );
}

#[test]
fn division_rounds_once_and_refuses_zero() {
    let one = Fixed::ONE;
    let three = Fixed::from_int(3).unwrap();
    assert_eq!(
        one.checked_div(three, Rounding::HalfEven),
        Some(Fixed::from_raw(333_333_333))
    );
    assert_eq!(
        one.checked_div(three, Rounding::Ceiling),
        Some(Fixed::from_raw(333_333_334))
    );
    assert_eq!(
        Fixed::from_int(2)
            .unwrap()
            .checked_div(three, Rounding::HalfEven),
        Some(Fixed::from_raw(666_666_667))
    );
    assert!(one.checked_div(Fixed::ZERO, Rounding::HalfEven).is_none());
    assert!(
        Fixed::MAX
            .checked_div(Fixed::from_raw(1), Rounding::HalfEven)
            .is_none()
    );
    assert_eq!(
        f(10, 0).checked_div_int(4, Rounding::HalfEven),
        Some(f(2, 500_000_000))
    );
    assert_eq!(
        Fixed::from_raw(5).checked_div_int(2, Rounding::HalfEven),
        Some(Fixed::from_raw(2))
    );
    assert_eq!(
        Fixed::from_raw(5).checked_div_int(2, Rounding::HalfUp),
        Some(Fixed::from_raw(3))
    );
    assert!(one.checked_div_int(0, Rounding::HalfEven).is_none());
}

#[test]
fn rounding_to_fewer_decimals() {
    let x = f(100, 123_456_789);
    assert_eq!(x.round(2, Rounding::HalfEven), f(100, 120_000_000));
    assert_eq!(x.round(2, Rounding::Up), f(100, 130_000_000));
    assert_eq!(x.round(0, Rounding::Floor), f(100, 0));
    assert_eq!(x.round(9, Rounding::HalfEven), x);
    assert_eq!(x.round(12, Rounding::HalfEven), x);
    let y = f(-100, 125_000_000);
    assert_eq!(y.round(2, Rounding::HalfEven), f(-100, 120_000_000));
    assert_eq!(y.round(2, Rounding::HalfUp), f(-100, 130_000_000));
    assert_eq!(y.round(2, Rounding::Floor), f(-100, 130_000_000));
    assert_eq!(y.round(2, Rounding::Ceiling), f(-100, 120_000_000));
}

#[test]
fn tick_grid_round_check_and_count() {
    let cent = Fixed::from_raw(10_000_000);
    let quarter = f(0, 250_000_000);
    let px = f(100, 254_000_000);
    assert_eq!(
        px.round_to_tick(cent, Rounding::HalfEven),
        Some(f(100, 250_000_000))
    );
    assert_eq!(
        px.round_to_tick(cent, Rounding::Ceiling),
        Some(f(100, 260_000_000))
    );
    assert_eq!(
        px.round_to_tick(quarter, Rounding::HalfEven),
        Some(f(100, 250_000_000))
    );
    assert_eq!(
        f(100, 375_000_000).round_to_tick(quarter, Rounding::HalfEven),
        Some(f(100, 500_000_000))
    );
    assert_eq!(
        f(100, 125_000_000).round_to_tick(quarter, Rounding::HalfEven),
        Some(f(100, 0))
    );
    assert!(f(100, 250_000_000).is_on_tick(cent));
    assert!(!px.is_on_tick(cent));
    assert!(!px.is_on_tick(Fixed::ZERO));
    assert!(px.round_to_tick(Fixed::ZERO, Rounding::HalfEven).is_none());
    assert!(
        px.round_to_tick(Fixed::from_raw(-1), Rounding::HalfEven)
            .is_none()
    );
    assert_eq!(f(100, 250_000_000).ticks(cent), Ok(10_025));
    assert_eq!(px.ticks(cent), Err(FixedError::OffTick));
    assert_eq!(px.ticks(Fixed::ZERO), Err(FixedError::InvalidTick));
    assert!(
        Fixed::MAX
            .round_to_tick(Fixed::from_raw(10), Rounding::Up)
            .is_none()
    );
}

#[test]
fn scale_conversion_in_and_out() {
    // 4dp FIX price 1002500 -> 100.25
    assert_eq!(
        Fixed::from_scaled(1_002_500, 4, Rounding::HalfEven),
        Some(f(100, 250_000_000))
    );
    assert_eq!(
        f(100, 250_000_000).to_scaled(4, Rounding::HalfEven),
        Some(1_002_500)
    );
    assert_eq!(
        f(100, 255_000_000).to_scaled(2, Rounding::HalfEven),
        Some(10_026)
    );
    assert_eq!(
        f(100, 255_000_000).to_scaled(2, Rounding::Down),
        Some(10_025)
    );
    // 12dp input is rounded into nine
    assert_eq!(
        Fixed::from_scaled(1_000_000_000_500, 12, Rounding::HalfEven),
        Some(Fixed::from_raw(SCALE))
    );
    assert_eq!(
        Fixed::from_scaled(1_000_000_000_500, 12, Rounding::HalfUp),
        Some(Fixed::from_raw(SCALE + 1))
    );
    assert_eq!(
        Fixed::ONE.to_scaled(12, Rounding::HalfEven),
        Some(1_000_000_000_000)
    );
    assert!(Fixed::from_scaled(1, 19, Rounding::HalfEven).is_none());
    assert!(Fixed::ONE.to_scaled(19, Rounding::HalfEven).is_none());
    assert!(Fixed::from_scaled(i64::MAX, 0, Rounding::HalfEven).is_none());
    assert!(Fixed::MAX.to_scaled(18, Rounding::HalfEven).is_none());
}

#[test]
fn float_boundary_is_exact_on_representable_values_and_rounds_the_rest() {
    assert_eq!(
        Fixed::from_f64(100.25, Rounding::HalfEven),
        Ok(f(100, 250_000_000))
    );
    assert_eq!(
        Fixed::from_f64(-0.5, Rounding::HalfEven),
        Ok(Fixed::from_raw(-500_000_000))
    );
    assert_eq!(Fixed::from_f64(0.0, Rounding::HalfEven), Ok(Fixed::ZERO));
    assert_eq!(Fixed::from_f64(-0.0, Rounding::HalfEven), Ok(Fixed::ZERO));
    // 0.1 is 0.1000000000000000055511151231257827... in binary: rounds to 0.1
    assert_eq!(
        Fixed::from_f64(0.1, Rounding::HalfEven),
        Ok(f(0, 100_000_000))
    );
    // ... and Up sees the excess and goes to the next billionth.
    assert_eq!(
        Fixed::from_f64(0.1, Rounding::Up),
        Ok(Fixed::from_raw(100_000_001))
    );
    // Below half a billionth every nearest mode gives zero; Up does not.
    assert_eq!(Fixed::from_f64(1e-12, Rounding::HalfEven), Ok(Fixed::ZERO));
    assert_eq!(Fixed::from_f64(1e-12, Rounding::Up), Ok(Fixed::from_raw(1)));
    assert_eq!(
        Fixed::from_f64(1e-300, Rounding::Ceiling),
        Ok(Fixed::from_raw(1))
    );
    assert_eq!(
        Fixed::from_f64(-1e-300, Rounding::Floor),
        Ok(Fixed::from_raw(-1))
    );
    assert_eq!(Fixed::from_f64(5e-324, Rounding::HalfEven), Ok(Fixed::ZERO));
    assert_eq!(
        Fixed::from_f64(f64::NAN, Rounding::HalfEven),
        Err(FixedError::NotFinite)
    );
    assert_eq!(
        Fixed::from_f64(f64::INFINITY, Rounding::HalfEven),
        Err(FixedError::NotFinite)
    );
    assert_eq!(
        Fixed::from_f64(1e10, Rounding::HalfEven),
        Err(FixedError::Overflow)
    );
    assert_eq!(
        Fixed::from_f64(1e300, Rounding::HalfEven),
        Err(FixedError::Overflow)
    );
    assert_eq!(
        Fixed::from_f64(9.0e9, Rounding::HalfEven),
        Fixed::from_int(9_000_000_000).ok_or(FixedError::Overflow)
    );
    assert_eq!(f(100, 250_000_000).to_f64(), 100.25);
    assert_eq!(
        Fixed::from_f64(123456.789, Rounding::HalfEven)
            .unwrap()
            .to_f64(),
        123456.789
    );
}

#[test]
fn parts_sign_and_predicates() {
    let x = f(-3, 250_000_000);
    assert_eq!(x.int_part(), -3);
    assert_eq!(x.frac_part(), -250_000_000);
    assert!(x.is_negative());
    assert!(!x.is_positive());
    assert!(!x.is_zero());
    assert_eq!(x.signum(), -1);
    assert_eq!(Fixed::ZERO.signum(), 0);
    assert!(Fixed::ZERO.is_zero());
    assert!(Fixed::ONE.is_positive());
}

#[test]
fn display_is_the_shortest_exact_decimal() {
    assert_eq!(f(100, 500_000_000).to_string(), "100.5");
    assert_eq!(f(100, 0).to_string(), "100");
    assert_eq!(Fixed::from_raw(1).to_string(), "0.000000001");
    assert_eq!(Fixed::from_raw(-1).to_string(), "-0.000000001");
    assert_eq!(f(-1, 250_000_000).to_string(), "-1.25");
    assert_eq!(Fixed::ZERO.to_string(), "0");
    assert_eq!(Fixed::MAX.to_string(), "9223372036.854775807");
    assert_eq!(Fixed::MIN.to_string(), "-9223372036.854775808");
    assert_eq!(format!("{:?}", f(1, 100_000_000)), "Fixed(1.1)");
}

#[test]
fn stress_random_products_match_a_128_bit_oracle() {
    let mut s = 0x1234_5678_9abc_def0u64;
    let mut next = || {
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        s
    };
    for _ in 0..200_000 {
        let a = Fixed::from_raw((next() % 2_000_000_000_000) as i64 - 1_000_000_000_000);
        let b = Fixed::from_raw((next() % 20_000_000_000) as i64 - 10_000_000_000);
        let exact = a.raw() as i128 * b.raw() as i128;
        let got = a.checked_mul(b, Rounding::Floor).unwrap().raw() as i128;
        assert_eq!(got, exact.div_euclid(SCALE as i128), "{a:?} * {b:?}");
        let back = a.checked_add(b).unwrap().checked_sub(b).unwrap();
        assert_eq!(back, a);
        if !b.is_zero() {
            let q = a.checked_div(b, Rounding::Down).unwrap();
            let r = a
                .checked_sub(q.checked_mul(b, Rounding::Down).unwrap())
                .unwrap();
            assert!(
                r.raw().unsigned_abs() <= b.raw().unsigned_abs() + 1,
                "{a:?} / {b:?}"
            );
        }
    }
}
