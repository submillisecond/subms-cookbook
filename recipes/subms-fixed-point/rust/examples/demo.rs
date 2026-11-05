use subms_fixed_point::{Fixed, Rounding};

fn main() {
    let px = Fixed::from_parts(100, 250_000_000).unwrap();
    let qty = Fixed::from_int(300).unwrap();
    let tick = Fixed::from_raw(10_000_000);

    let notional = px.checked_mul(qty, Rounding::HalfEven).unwrap();
    println!("{px} x {qty} = {notional}");

    let off = Fixed::from_parts(100, 254_000_000).unwrap();
    println!(
        "{off} on a {tick} grid: {} (half-even), {} (floor), on grid: {}",
        off.round_to_tick(tick, Rounding::HalfEven).unwrap(),
        off.round_to_tick(tick, Rounding::Floor).unwrap(),
        off.is_on_tick(tick)
    );

    let fills: Vec<Fixed> = (0..10)
        .map(|_| Fixed::from_parts(0, 100_000_000).unwrap())
        .collect();
    let total: Fixed = fills.iter().sum();
    println!("ten fills of 0.1 sum to {total} exactly");
}
