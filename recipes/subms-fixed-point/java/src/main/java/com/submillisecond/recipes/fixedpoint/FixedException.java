package com.submillisecond.recipes.fixedpoint;

/** The typed failure surface. */
public final class FixedException extends ArithmeticException {

    private static final long serialVersionUID = 1L;

    /** Mirrors the Rust port's {@code FixedError} variant for variant. */
    public enum Kind {
        /** The result does not fit the 64-bit raw, or an intermediate does not fit 128 bits. */
        OVERFLOW("fixed-point overflow"),
        /** A divisor or a tick of zero. */
        DIVISION_BY_ZERO("division by zero"),
        /** The value is not an integer multiple of the tick. */
        OFF_TICK("value is not on the tick grid"),
        /** A tick that is zero or negative. */
        INVALID_TICK("tick must be positive"),
        /** Text that is not {@code [-]digits[.digits]}, or has no digits at all. */
        INVALID_TEXT("not a decimal number"),
        /** More fractional digits than the type holds, or than a conversion can express. */
        TOO_PRECISE("more precision than the type holds"),
        /** A double that is NaN or infinite. */
        NOT_FINITE("double is NaN or infinite"),
        /** Two amounts in different currencies met where one currency is required. */
        CURRENCY_MISMATCH("currencies differ"),
        /** A currency code that is not three ASCII letters. */
        INVALID_CURRENCY("currency code must be three ASCII letters"),
        /** The caller's output buffer is too small for the rendered text. */
        BUFFER_TOO_SMALL("output buffer too small");

        private final String message;

        Kind(String message) {
            this.message = message;
        }

        public String message() {
            return message;
        }
    }

    private final Kind kind;

    public FixedException(Kind kind) {
        super(kind.message());
        this.kind = kind;
    }

    public Kind kind() {
        return kind;
    }
}
