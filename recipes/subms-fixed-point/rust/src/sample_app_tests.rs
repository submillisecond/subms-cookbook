//! Pins the behaviour each section of the `sample_app` example demonstrates: the fill ledger nets exactly and order-independently, and each optional feature holds its own contract.

use super::*;

struct Fills {
    state: u64,
    left: usize,
}

impl Fills {
    fn new(seed: u64, count: usize) -> Fills {
        Fills {
            state: seed.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1),
            left: count,
        }
    }
    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state >> 11
    }
}

impl Iterator for Fills {
    type Item = (Fixed, Fixed);
    fn next(&mut self) -> Option<(Fixed, Fixed)> {
        if self.left == 0 {
            return None;
        }
        self.left -= 1;
        let cents = 9_950 + (self.next_u64() % 101) as i64;
        let px = Fixed::from_raw(cents * 10_000_000);
        let lots = 100 + (self.next_u64() % 4_901) as i64;
        let signed = if self.next_u64() % 2 == 0 {
            lots
        } else {
            -lots
        };
        Some((px, Fixed::from_int(signed).unwrap()))
    }
}

#[test]
fn base_ledger_nets_exactly_and_independent_of_fill_order() {
    let tick = Fixed::from_raw(10_000_000);
    let fills: Vec<(Fixed, Fixed)> = Fills::new(7, 50_000).collect();
    let net = |fs: &[(Fixed, Fixed)]| {
        let mut position = Fixed::ZERO;
        let mut cash = Fixed::ZERO;
        for (px, qty) in fs {
            assert!(px.is_on_tick(tick));
            position += *qty;
            cash -= px.checked_mul(*qty, Rounding::HalfEven).unwrap();
        }
        (position, cash)
    };
    let forward = net(&fills);
    let mut reversed = fills.clone();
    reversed.reverse();
    assert_eq!(net(&reversed), forward);
    assert!(
        forward.1.is_on_tick(tick),
        "cash of on-grid fills is on grid"
    );

    let mut cash_f = 0.0f64;
    for (px, qty) in &fills {
        cash_f -= px.to_f64() * qty.to_f64();
    }
    let mut cash_r = 0.0f64;
    for (px, qty) in &reversed {
        cash_r -= px.to_f64() * qty.to_f64();
    }
    assert_ne!(cash_f, cash_r, "the f64 ledger depends on fill order");
}

#[cfg(feature = "text")]
#[test]
fn text_section_accepts_the_wire_shapes_and_refuses_excess_precision() {
    assert_eq!(Fixed::parse("100.25").unwrap().raw(), 100_250_000_000);
    assert_eq!(Fixed::parse("0100.250").unwrap().raw(), 100_250_000_000);
    assert_eq!(Fixed::parse("1e3"), Err(FixedError::InvalidText));
    assert_eq!(Fixed::parse("100.0000000001"), Err(FixedError::TooPrecise));
    let total: Fixed = Fills::new(7, 1_000)
        .map(|(px, qty)| px.checked_mul(qty, Rounding::HalfEven).unwrap())
        .sum();
    let mut buf = [0u8; 32];
    let n2 = total.write_to(&mut buf, 2).unwrap();
    let two = std::str::from_utf8(&buf[..n2]).unwrap().to_string();
    let n9 = total.write_to(&mut buf, 9).unwrap();
    let nine = std::str::from_utf8(&buf[..n9]).unwrap().to_string();
    assert!(nine.starts_with(&two), "{nine} starts with {two}");
    assert_eq!(Fixed::parse(&nine).unwrap(), total);
}

#[cfg(feature = "money")]
#[test]
fn money_section_refuses_cross_currency_and_settles_per_minor_unit() {
    let usd = Money::new(
        Fixed::from_parts(1_250_000, 375_000_000).unwrap(),
        Currency::USD,
    );
    let eur = Money::new(
        Fixed::from_parts(-840_000, 125_000_000).unwrap(),
        Currency::EUR,
    );
    assert_eq!(usd.checked_add(eur), Err(FixedError::CurrencyMismatch));
    let rate = Fixed::from_parts(1, 84_500_000).unwrap();
    let net = usd
        .checked_add(
            eur.convert(rate, Currency::USD, Rounding::HalfEven)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(net.currency(), Currency::USD);
    assert_eq!(
        net.amount().raw(),
        1_250_000_375_000_000 - 910_980_135_562_500
    );
    assert!(!net.is_settleable());
    assert!(net.round_to_minor(Rounding::HalfEven).is_settleable());
    let jpy = Money::new(
        Fixed::from_parts(12_345_678, 900_000_000).unwrap(),
        Currency::JPY,
    );
    assert_eq!(
        jpy.round_to_minor(Rounding::HalfEven).amount(),
        Fixed::from_int(12_345_679).unwrap()
    );
}

#[cfg(feature = "wide")]
#[test]
fn wide_section_holds_a_session_the_base_type_cannot() {
    let mut gross = Fixed128::ZERO;
    let mut narrow = Some(Fixed::ZERO);
    let mut overflowed_at = None;
    for (k, (px, qty)) in Fills::new(11, 1_000_000).enumerate() {
        let lots = qty.checked_abs().unwrap();
        gross = gross.checked_add_product(px, lots).unwrap();
        if let Some(s) = narrow {
            narrow = px
                .checked_mul(lots, Rounding::HalfEven)
                .and_then(|n| s.checked_add(n));
            if narrow.is_none() {
                overflowed_at = Some(k + 1);
            }
        }
    }
    assert!(!gross.is_zero());
    assert!(
        overflowed_at.is_some(),
        "a million fills around 100 x 2500 lots passes 9.2 billion"
    );
    assert_eq!(
        gross.to_fixed(Rounding::HalfEven),
        Err(FixedError::Overflow)
    );
    let avg = gross
        .checked_div_int(1_000_000, Rounding::HalfEven)
        .unwrap()
        .to_fixed(Rounding::HalfEven)
        .unwrap();
    assert!(avg > Fixed::from_int(200_000).unwrap() && avg < Fixed::from_int(300_000).unwrap());
}
