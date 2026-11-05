//! Sample app: a tour of `subms-fixed-point`, base API first, then each optional feature.

use subms_fixed_point::{Fixed, Rounding};

/// Deterministic fill stream.
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

fn main() {
    base_fill_ledger();

    #[cfg(feature = "text")]
    text_wire_boundary();

    #[cfg(feature = "money")]
    money_two_currency_book();

    #[cfg(feature = "wide")]
    wide_session_notional();
}

/// Base API: net 50,000 fills into position and cash.
fn base_fill_ledger() {
    println!("== base: netting a day of fills ==");
    let tick = Fixed::from_raw(10_000_000);

    let mut position = Fixed::ZERO;
    let mut cash = Fixed::ZERO;
    let mut position_f = 0.0f64;
    let mut cash_f = 0.0f64;
    let mut off_grid = 0usize;

    for (px, qty) in Fills::new(7, 50_000) {
        if !px.is_on_tick(tick) {
            off_grid += 1;
        }
        position += qty;
        cash -= px.checked_mul(qty, Rounding::HalfEven).unwrap();
        position_f += qty.to_f64();
        cash_f -= px.to_f64() * qty.to_f64();
    }

    println!("  fills 50000, off-grid prices {off_grid}");
    println!("  position {position} lots, cash {cash}");
    println!("  f64 ledger position {position_f:.1}, cash {cash_f:.9}");
    let drift = cash.to_f64() - cash_f;
    println!("  f64 drift vs exact: {drift:.9} (sign and size depend on fill order)");

    let mark = Fixed::from_parts(100, 10_000_000).unwrap();
    let mtm = position.checked_mul(mark, Rounding::HalfEven).unwrap();
    let pnl = cash + mtm;
    println!("  mark {mark}, position value {mtm}, pnl {pnl}");

    let avg = cash
        .checked_neg()
        .unwrap()
        .checked_div(position, Rounding::HalfEven)
        .map(|a| a.to_string())
        .unwrap_or_else(|| "n/a (flat)".to_string());
    println!("  average price {avg}");
    println!(
        "  rounded to the tick: {} (half-even) {} (half-up) {} (floor)",
        cash.round_to_tick(tick, Rounding::HalfEven).unwrap(),
        cash.round_to_tick(tick, Rounding::HalfUp).unwrap(),
        cash.round_to_tick(tick, Rounding::Floor).unwrap()
    );
}

/// `text`: prices come off the wire as ASCII and go back to the clearer as ASCII at a fixed number of decimals.
#[cfg(feature = "text")]
fn text_wire_boundary() {
    use subms_fixed_point::FixedError;

    println!("== text: the wire boundary ==");
    let wire = [
        "100.25",
        "99.9",
        "-0.000000001",
        "0100.250",
        "1e3",
        "100.0000000001",
    ];
    for w in wire {
        match Fixed::parse(w) {
            Ok(v) => println!("  {w:>16} -> {v}"),
            Err(FixedError::TooPrecise) => println!("  {w:>16} -> rejected, finer than 9 decimals"),
            Err(e) => println!("  {w:>16} -> rejected, {e}"),
        }
    }

    let total: Fixed = Fills::new(7, 1_000)
        .map(|(px, qty)| px.checked_mul(qty, Rounding::HalfEven).unwrap())
        .sum();
    let mut buf = [0u8; 32];
    let n = total.write_to(&mut buf, 2).unwrap();
    println!(
        "  clearer file, 2dp: {}",
        std::str::from_utf8(&buf[..n]).unwrap()
    );
    let n = total.write_to(&mut buf, 9).unwrap();
    println!(
        "  audit file, 9dp:   {}",
        std::str::from_utf8(&buf[..n]).unwrap()
    );
    let n = total
        .round(2, Rounding::HalfEven)
        .write_to(&mut buf, 2)
        .unwrap();
    println!(
        "  rounded first:     {}",
        std::str::from_utf8(&buf[..n]).unwrap()
    );
}

/// `money`: a book with a USD leg and a EUR leg.
#[cfg(feature = "money")]
fn money_two_currency_book() {
    use subms_fixed_point::{Currency, FixedError, Money};

    println!("== money: a two-currency book ==");
    let usd = Money::new(
        Fixed::from_parts(1_250_000, 375_000_000).unwrap(),
        Currency::USD,
    );
    let eur = Money::new(
        Fixed::from_parts(-840_000, 125_000_000).unwrap(),
        Currency::EUR,
    );
    let jpy = Money::new(
        Fixed::from_parts(12_345_678, 900_000_000).unwrap(),
        Currency::JPY,
    );

    match usd.checked_add(eur) {
        Err(FixedError::CurrencyMismatch) => {
            println!("  {usd} + {eur}: refused, currencies differ")
        }
        other => println!("  unexpected {other:?}"),
    }
    let rate = Fixed::from_parts(1, 84_500_000).unwrap();
    let eur_in_usd = eur
        .convert(rate, Currency::USD, Rounding::HalfEven)
        .unwrap();
    let net = usd.checked_add(eur_in_usd).unwrap();
    println!("  EUR leg at {rate}: {eur_in_usd}");
    println!("  net USD: {net}");
    println!(
        "  settle: {} / {} / {}",
        net.round_to_minor(Rounding::HalfEven),
        jpy.round_to_minor(Rounding::HalfEven),
        Money::new(Fixed::from_parts(10, 123_456_789).unwrap(), Currency::BHD)
            .round_to_minor(Rounding::HalfEven)
    );
    println!(
        "  settleable as quoted: USD {} JPY {}",
        net.is_settleable(),
        jpy.is_settleable()
    );
}

/// `wide`: a session's notional on one liquid book passes the 9.2 billion the base type holds.
#[cfg(feature = "wide")]
fn wide_session_notional() {
    use subms_fixed_point::Fixed128;

    println!("== wide: a session's notional ==");
    let mut gross = Fixed128::ZERO;
    let mut fills = 0u64;
    let mut overflowed_at: Option<u64> = None;
    let mut narrow_sum = Fixed::ZERO;

    for (px, qty) in Fills::new(11, 1_000_000) {
        let lots = qty.checked_abs().unwrap();
        gross = gross.checked_add_product(px, lots).unwrap();
        fills += 1;
        if overflowed_at.is_none() {
            match px
                .checked_mul(lots, Rounding::HalfEven)
                .and_then(|n| narrow_sum.checked_add(n))
            {
                Some(s) => narrow_sum = s,
                None => overflowed_at = Some(fills),
            }
        }
    }

    println!("  fills {fills}");
    println!("  gross notional (exact, 18dp): {gross}");
    println!(
        "  as Fixed: {}",
        gross
            .to_fixed(Rounding::HalfEven)
            .map(|f| f.to_string())
            .unwrap_or_else(|e| format!("does not fit ({e})"))
    );
    match overflowed_at {
        Some(k) => println!("  a 64-bit running sum overflowed at fill {k}"),
        None => println!("  a 64-bit running sum held: {narrow_sum}"),
    }
    let avg = gross
        .checked_div_int(fills as i64, Rounding::HalfEven)
        .unwrap()
        .to_fixed(Rounding::HalfEven)
        .unwrap();
    println!("  average notional per fill {avg}");
}
