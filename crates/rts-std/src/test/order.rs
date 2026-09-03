//! Numeric matchers — `toBeGreaterThan`, `toBeGreaterThanOrEqual`,
//! `toBeLessThan`, `toBeLessThanOrEqual`, `toBeCloseTo`, `toBeNaN`,
//! `toBeFinite`.
//!
//! The four comparisons reuse `rts_core::entry::greater`/`less` and their
//! `_equal` forms — the actual `<`/`>`/`<=`/`>=` operators the compiler emits
//! for a comparison it could not resolve at compile time (`entry/operators.rs`).
//! They already run the specification's `AbstractRelationalComparison`, which
//! is the one place a JavaScript comparison decides between a lexicographic
//! and a numeric answer from what its operands turn out to be — reimplementing
//! that here, even to call the same string-vs-number split, would be a second
//! copy of a decision `rts-core` already owns.

/// The default precision `toBeCloseTo` uses when none is given — Jest's own
/// default, so a call with one argument means the same thing here as there.
const DEFAULT_PRECISION: f64 = 2.0;

/// `expect(x).toBeGreaterThan(y)`.
pub(super) extern "C" fn to_be_greater_than(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::greater(received, expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toBeGreaterThanOrEqual(y)`.
pub(super) extern "C" fn to_be_greater_than_or_equal(
    _e: u64,
    this: u64,
    expected: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::greater_equal(received, expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toBeLessThan(y)`.
pub(super) extern "C" fn to_be_less_than(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::less(received, expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toBeLessThanOrEqual(y)`.
pub(super) extern "C" fn to_be_less_than_or_equal(
    _e: u64,
    this: u64,
    expected: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::less_equal(received, expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toBeCloseTo(y, precision)` — Jest's own tolerance formula:
/// passes when `|received - expected| < 10^-precision / 2`. `precision`
/// defaults to [`DEFAULT_PRECISION`] when the call omits it, read the way
/// `toBeUndefined` reads an absent value in `equality.rs` — compared to the
/// raw `undefined` bit pattern rather than coerced, because coercing
/// `undefined` through `ToNumber` answers `NaN` and would make every
/// unspecified precision fail the way a precision of `NaN` should, not the way
/// an omitted one should.
pub(super) extern "C" fn to_be_close_to(_e: u64, this: u64, expected: u64, precision: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let digits = if precision == rts_core::entry::undefined_value() {
        DEFAULT_PRECISION
    } else {
        super::to_number(precision)
    };
    let threshold = 10f64.powf(-digits) / 2.0;
    let held = (super::to_number(received) - super::to_number(expected)).abs() < threshold;
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toBeNaN()`.
pub(super) extern "C" fn to_be_nan(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = super::to_number(received).is_nan();
    super::settle(super::negate_if(this, held), super::is_negated(this), received, received)
}

/// `expect(x).toBeFinite()`.
pub(super) extern "C" fn to_be_finite(_e: u64, this: u64, _a0: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = super::to_number(received).is_finite();
    super::settle(super::negate_if(this, held), super::is_negated(this), received, received)
}
