//! Content matchers — `toContain`, `toStartWith`, `toEndWith`, `toHaveLength`.
//!
//! The string forms go through `rts_core::entry::text_of`, which is `ToString`
//! rather than a raw byte read — the same reason `console`'s formatter uses it
//! — and Rust's `str::contains`/`starts_with`/`ends_with` are themselves
//! Unicode-scalar substring tests over UTF-8, so a multi-byte needle (`"ç"`
//! inside `"ação"`) compares by content and never by byte offset.

/// `expect(x).toContain(y)`.
///
/// Two shapes, decided by what `x` is — the same split real Jest makes.
/// `Array.prototype.includes` in the real harness uses `SameValueZero`, which
/// differs from the `SameValue` used here only on `+0` vs `-0`; nothing in this
/// matcher set needs that distinction, so [`element_matches`] shares
/// `same_value` with `toBe` rather than a second identity test that would
/// answer differently for a case this corpus never asks about.
pub(super) extern "C" fn to_contain(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = if rts_core::entry::is_array(received) {
        element_matches(received, expected)
    } else {
        rts_core::entry::text_of(received)
            .unwrap_or_default()
            .contains(&rts_core::entry::text_of(expected).unwrap_or_default())
    };
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// Whether `expected` is an element of `array`, by [`rts_core::entry::same_value`].
fn element_matches(array: u64, expected: u64) -> bool {
    let length = rts_core::entry::array_length(array) as usize;
    (0..length).any(|index| {
        let key = rts_core::entry::make_number(index as f64);
        let element = rts_core::entry::element_at(array, key);
        rts_core::entry::same_value(element, expected)
    })
}

/// `expect(x).toStartWith(y)`.
pub(super) extern "C" fn to_start_with(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::text_of(received)
        .unwrap_or_default()
        .starts_with(&rts_core::entry::text_of(expected).unwrap_or_default());
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toEndWith(y)`.
pub(super) extern "C" fn to_end_with(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let held = rts_core::entry::text_of(received)
        .unwrap_or_default()
        .ends_with(&rts_core::entry::text_of(expected).unwrap_or_default());
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}

/// `expect(x).toHaveLength(n)`.
///
/// An array's own length when `x` is one. Otherwise `x` is read as the length
/// already, through [`super::to_number`] — this corpus's own convention,
/// stated by `tests/rts_test_matchers.test.ts` itself: every actual crosses
/// `expect()` in whatever form the caller chose, and this matcher's callers
/// write `expect(`${s.length}`).toHaveLength(n)` rather than
/// `expect(s).toHaveLength(n)`, so what arrives here for a non-array is a
/// number already computed, carried as text. Measuring `x` itself in that case
/// — reading a STRING's own `.length` — would answer the length of the digits,
/// not the length the caller meant, which is the wrong number for the only
/// caller this matcher has.
pub(super) extern "C" fn to_have_length(_e: u64, this: u64, expected: u64, _a1: u64, _a2: u64, _a3: u64) -> u64 {
    let received = super::received_of(this);
    let actual = if rts_core::entry::is_array(received) {
        rts_core::entry::array_length(received)
    } else {
        super::to_number(received)
    };
    let held = actual == super::to_number(expected);
    super::settle(super::negate_if(this, held), super::is_negated(this), received, expected)
}
