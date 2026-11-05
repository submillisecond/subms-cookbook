use super::*;

fn all() -> [Rounding; 7] {
    [
        Rounding::HalfEven,
        Rounding::HalfUp,
        Rounding::HalfDown,
        Rounding::Down,
        Rounding::Up,
        Rounding::Floor,
        Rounding::Ceiling,
    ]
}

#[test]
fn exact_quotient_is_untouched_by_every_mode() {
    for m in all() {
        assert_eq!(m.div(100, 10), 10, "{m:?}");
        assert_eq!(m.div(-100, 10), -10, "{m:?}");
        assert_eq!(m.div(0, 7), 0, "{m:?}");
    }
}

#[test]
fn ties_follow_java_rounding_mode_semantics() {
    // 2.5 and -2.5, 3.5 and -3.5: the BigDecimal javadoc table.
    let cases: [(Rounding, [i128; 4]); 7] = [
        (Rounding::HalfEven, [2, -2, 4, -4]),
        (Rounding::HalfUp, [3, -3, 4, -4]),
        (Rounding::HalfDown, [2, -2, 3, -3]),
        (Rounding::Down, [2, -2, 3, -3]),
        (Rounding::Up, [3, -3, 4, -4]),
        (Rounding::Floor, [2, -3, 3, -4]),
        (Rounding::Ceiling, [3, -2, 4, -3]),
    ];
    for (m, want) in cases {
        assert_eq!(m.div(25, 10), want[0], "{m:?} 2.5");
        assert_eq!(m.div(-25, 10), want[1], "{m:?} -2.5");
        assert_eq!(m.div(35, 10), want[2], "{m:?} 3.5");
        assert_eq!(m.div(-35, 10), want[3], "{m:?} -3.5");
    }
}

#[test]
fn non_ties_round_to_nearest_under_the_half_modes() {
    for m in [Rounding::HalfEven, Rounding::HalfUp, Rounding::HalfDown] {
        assert_eq!(m.div(26, 10), 3, "{m:?}");
        assert_eq!(m.div(24, 10), 2, "{m:?}");
        assert_eq!(m.div(-26, 10), -3, "{m:?}");
        assert_eq!(m.div(-24, 10), -2, "{m:?}");
    }
}

#[test]
fn directional_modes_ignore_how_close_the_remainder_is() {
    assert_eq!(Rounding::Up.div(21, 10), 3);
    assert_eq!(Rounding::Up.div(-21, 10), -3);
    assert_eq!(Rounding::Down.div(29, 10), 2);
    assert_eq!(Rounding::Down.div(-29, 10), -2);
    assert_eq!(Rounding::Floor.div(29, 10), 2);
    assert_eq!(Rounding::Floor.div(-21, 10), -3);
    assert_eq!(Rounding::Ceiling.div(21, 10), 3);
    assert_eq!(Rounding::Ceiling.div(-29, 10), -2);
}

#[test]
fn negative_divisor_follows_the_mathematical_quotient() {
    assert_eq!(Rounding::Floor.div(7, -2), -4);
    assert_eq!(Rounding::Ceiling.div(7, -2), -3);
    assert_eq!(Rounding::HalfEven.div(7, -2), -4);
    assert_eq!(Rounding::HalfEven.div(5, -2), -2);
    assert_eq!(Rounding::HalfUp.div(5, -2), -3);
}

#[test]
fn half_even_alternates_on_a_run_of_ties() {
    let got: Vec<i128> = (0..8)
        .map(|k| Rounding::HalfEven.div(2 * k + 1, 2))
        .collect();
    assert_eq!(got, vec![0, 2, 2, 4, 4, 6, 6, 8]);
}

#[test]
fn extreme_magnitudes_do_not_overflow_the_tie_test() {
    let n = i128::MAX;
    assert_eq!(Rounding::HalfEven.div(n, 2), n / 2 + 1);
    assert_eq!(Rounding::Down.div(i128::MIN + 1, 3), (i128::MIN + 1) / 3);
}

#[test]
fn names_match_java_rounding_mode() {
    let names: Vec<&str> = all().iter().map(|m| m.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "HALF_EVEN",
            "HALF_UP",
            "HALF_DOWN",
            "DOWN",
            "UP",
            "FLOOR",
            "CEILING"
        ]
    );
    assert_eq!(Rounding::default(), Rounding::HalfEven);
}
