//! `n.toString(radix)` and `n.toFixed(digits)` over a number the compiler
//! already holds unboxed, with no method dispatch.
//!
//! `i.toString()` cost 170 ns and `x.toFixed(2)` 200 after `fixed.rs`, of
//! which the text itself is 16 to 60: the rest is the call — a property read
//! that walks from the primitive to `Number.prototype`, a native dispatch,
//! the receiver unwrapped, the argument slots rebuilt. Where the receiver is
//! a PROVEN double and the whole program leaves `Number` alone (the same
//! `only_a_base` proof `Number.isNaN` rests on), both emitters call one of
//! these with the double itself. Nothing about the answer changes: the bodies
//! below ARE the members' — `class.rs` calls the same two functions — so the
//! direct form and the method cannot disagree about a radix, a range or a
//! rounding.
//!
//! A receiver the compiler has not proved a number takes the member, so a
//! wrapper object, a patched instance or a value that only looks numeric is
//! never here.

use super::super::objects::undefined_of;
use super::super::{class_support, throw, with_current};
use super::{format, parse};

/// `n.toString(radix)` — `radix` absent is base ten.
#[rtse::entry]
pub fn number_to_string_direct(number: f64, radix: u64) -> u64 {
    spelled_in_radix(number, radix)
}

/// `n.toFixed(digits)`.
#[rtse::entry]
pub fn number_to_fixed_direct(number: f64, digits: u64) -> u64 {
    fixed_text(number, class_support::to_number(digits))
}

/// The text of `Number.prototype.toString` for `number` and the radix VALUE
/// it was given — one definition, called by the member and by the door.
pub(super) fn spelled_in_radix(number: f64, radix: u64) -> u64 {
    let base = parse::radix_argument(radix);
    // Uma base fora de 2..=36 e um `RangeError`, e nao o decimal: responder
    // o decimal fazia `(5).toString(1)` responder `"5"`, que e uma resposta
    // certa para uma pergunta que o programa nao fez.
    //
    // ZERO esta dentro dessa recusa, e antes nao estava: a ausencia do
    // argumento era escrita como zero, entao `(5).toString(0)` nao tinha
    // como ser distinguido de `(5).toString()`. `radix_argument` responde
    // `None` so para a ausencia, que e o que separa os dois.
    if base.is_some_and(|base| !(2..=36).contains(&base)) {
        throw::range_error("toString() radix must be between 2 and 36");
        return with_current(|context| undefined_of(context));
    }
    with_current(|context| {
        // DECIMAL AND SMALL answers from the table. Only base ten: the cache is
        // keyed by the number, and `(255).toString(16)` is a different answer
        // for the same key — reading it from here would spell `"255"` for
        // `"ff"`, which is a wrong answer that runs.
        if matches!(base, None | Some(10))
            && let Some(cached) = context.small_number_text(number)
        {
            return cached;
        }
        let text = match base {
            None | Some(10) => crate::coerce::number_to_string(number),
            Some(base) => format::in_radix_str(number, base as u32),
        };
        context.intern_value(text).bits()
    })
}

/// The text of `Number.prototype.toFixed` for `number` and the digit count
/// already converted to a number — one definition, for the member and the door.
pub(super) fn fixed_text(number: f64, digits: f64) -> u64 {
    let asked = match digits.is_nan() {
        true => 0.0,
        false => digits.trunc(),
    };
    // Grampear era responder `"0.00"` a `(1).toFixed(-1)` e `100` casas a
    // `(1).toFixed(101)` — dois pedidos ilegais atendidos com um numero que
    // o programa nao pediu. O intervalo e da especificacao, e o `RangeError`
    // sai FORA do emprestimo porque construir o erro toma o contexto.
    let Some(places) = super::class::in_range(
        asked,
        0.0,
        100.0,
        "toFixed() digits argument must be between 0 and 100",
    ) else {
        return with_current(|context| undefined_of(context));
    };
    with_current(|context| {
        let text = match number.is_finite() && number.abs() < 1e21 {
            true => format::fixed_str(number, places),
            // Past 1e21 the specification falls back to the ordinary
            // `ToString`, which is why this is not a formatting width but a
            // branch.
            false => crate::coerce::number_to_string(number),
        };
        context.intern_value(text).bits()
    })
}
