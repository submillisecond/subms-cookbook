# subms-fixed-point (Rust)

Exact fixed-point decimal: `i64` at nine places, `i128` on the way through a multiply or divide.

## Representation

`Fixed(i64)` with `SCALE = 10^9`. Range a hair over +/- 9.2 billion whole units. Add, subtract, compare and hash are the integer instructions; multiply and divide widen to 128 bits, round once under the named `Rounding`, and narrow back. Overflow is `None` from the `checked_*` family and a panic from the operators.

## Implementation notes

- Seven rounding modes with `java.math.RoundingMode` tie semantics, so the Java port agrees digit for digit.
- `round_to_tick` / `is_on_tick` / `ticks` work on any positive tick, not only powers of ten.
- `from_scaled` / `to_scaled` move between the nine-place representation and a wire scale (FIX at 4, a venue at 7) exactly, or with one named rounding when precision is lost.
- `from_f64` decomposes the float into mantissa and exponent and rounds the exact rational as integers, so the conversion rounds once and the same way on every platform.
- Features: `text` (zero-allocation parse / write), `money` (currency-tagged amounts), `wide` (`Fixed128`, exact products and sums at eighteen places).

## Quality bar

- Reference impl: `rust_decimal`, `decimal4j`, `java.math.BigDecimal`; Goldberg (1991) for why floats are the wrong tool.
- Sub-ms claim under: `add`, `mul`, `div`, `round_tick` p99 < 1 ms across 100k ops on prices on a one-cent grid, single-threaded.
- Not claimed: arbitrary or per-value scale; transcendental functions; a currency table beyond the ISO 4217 exponent.

## Bench numbers

Filled in once the bench is run on the canonical hardware. The cookbook page carries the measured values - never invented.

## Consumed by

Nothing yet.
