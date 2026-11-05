package com.submillisecond.recipes.fixedpoint.features;

import com.submillisecond.recipes.fixedpoint.Fixed;
import com.submillisecond.recipes.fixedpoint.FixedException;
import com.submillisecond.recipes.fixedpoint.Rounding;

/** An amount in one currency. */
public record Money(Fixed amount, Currency currency) {

    public static Money zero(Currency currency) {
        return new Money(Fixed.ZERO, currency);
    }

    /** Same-currency sum. */
    public Money add(Money rhs) {
        requireSame(rhs);
        return new Money(amount.add(rhs.amount), currency);
    }

    public Money sub(Money rhs) {
        requireSame(rhs);
        return new Money(amount.sub(rhs.amount), currency);
    }

    /** Scale by a unitless factor: a quantity, a fee rate, a haircut. */
    public Money mul(Fixed factor, Rounding mode) {
        return new Money(amount.mul(factor, mode), currency);
    }

    /** Convert at {@code rate} units of {@code to} per unit of this currency. */
    public Money convert(Fixed rate, Currency to, Rounding mode) {
        return new Money(amount.mul(rate, mode), to);
    }

    /** Round to the currency's minor unit: cents, yen, fils. */
    public Money roundToMinor(Rounding mode) {
        return new Money(amount.round(currency.minorUnitDecimals(), mode), currency);
    }

    /** Whether the amount already sits on the minor-unit grid. */
    public boolean isSettleable() {
        return amount.round(currency.minorUnitDecimals(), Rounding.DOWN).equals(amount);
    }

    public boolean isZero() {
        return amount.isZero();
    }

    public boolean isNegative() {
        return amount.isNegative();
    }

    private void requireSame(Money rhs) {
        if (!currency.equals(rhs.currency)) {
            throw new FixedException(FixedException.Kind.CURRENCY_MISMATCH);
        }
    }

    /** {@code 100.5 USD}. */
    @Override
    public String toString() {
        return amount + " " + currency;
    }
}
