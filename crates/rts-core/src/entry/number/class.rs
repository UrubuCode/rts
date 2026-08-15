//! The two classes: what `Number` and `Boolean` install, and what their
//! prototypes answer.
//!
//! # How a method on a primitive is reached at all
//!
//! `Number.isNaN(x)` is on the constructor and needs nothing. `(5).toFixed(2)`
//! is a read on a *primitive receiver*, and a number is not a cell — it is the
//! encoding itself — so the chain walk that substitutes `String.prototype` for a
//! text cell has nothing to walk from.
//!
//! [`crate::entry::objects::get_property`] therefore starts the lookup on this
//! prototype directly when the receiver is a double or a boolean, and calls
//! whatever it finds with the primitive as the receiver. That is why every
//! member here reads its own `this` through [`receiver_number`] rather than
//! expecting an object: the receiver is a bare double far more often than it is
//! a wrapper, and no wrapper is ever made to satisfy a read.
//!
//! A wrapper the PROGRAM made — `new Number(5)` — is the other receiver, and it
//! answers the primitive it recorded when it was constructed. Both come out of
//! `receiver_number`, which is the point: a method that unwrapped for itself is
//! a method that would eventually forget to.
//!
//! # Why `Number.isNaN` does not coerce and `Number(x)` does
//!
//! They are different questions, and the language spells the difference
//! deliberately. The global `isNaN("abc")` is `true` because it converts first;
//! `Number.isNaN("abc")` is `false`, because the argument is not a number at
//! all. An implementation that coerced in both would make the second useless —
//! it exists precisely to be the one that does not.
//!
//! So these take `u64` and ask what arrived, where `Math`'s members take `f64`
//! and let the wrapper convert. The parameter type is the statement.
//!
//! # Why `parseInt` and `parseFloat` are NOT here
//!
//! They were, as two `#[stat]` members, and that made `Number.parseInt` a
//! second function object beside the global one — so `Number.parseInt === parseInt`
//! was `false`, which a program can see. [`super::register_number`] puts the
//! global's own cell on the constructor instead.

use super::super::objects::undefined_of;
use super::super::{bigint_class, class_support, primitive_proto, throw, with_current};
use super::{format, parse};
use crate::text::Str;
use crate::value::Value;

/// `Number`.
#[rtse::class("Number")]
impl Number {
    /// The largest integer a double represents exactly.
    #[stat]
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    /// The smallest.
    #[stat]
    const MIN_SAFE_INTEGER: f64 = -9_007_199_254_740_991.0;
    /// The largest finite double.
    #[stat]
    const MAX_VALUE: f64 = f64::MAX;
    /// The smallest positive double, subnormals included.
    #[stat]
    const MIN_VALUE: f64 = 5e-324;
    /// The difference between 1 and the next double above it.
    #[stat]
    const EPSILON: f64 = f64::EPSILON;
    /// `Number.POSITIVE_INFINITY`.
    #[stat]
    const POSITIVE_INFINITY: f64 = f64::INFINITY;
    /// `Number.NEGATIVE_INFINITY`.
    #[stat]
    const NEGATIVE_INFINITY: f64 = f64::NEG_INFINITY;
    /// `Number.NaN`.
    #[stat]
    const NaN: f64 = f64::NAN;

    /// `Number(x)` — the numeric value of an argument.
    ///
    /// `new Number(x)` answers the object `construct` made, because a primitive
    /// is not an object and a constructor returning one does not win. That
    /// object now REMEMBERS the number — `[[NumberData]]`, recorded beside the
    /// cell — which is what every method below reads.
    ///
    /// This used to be a stated divergence: the object was made and the number
    /// thrown away, on the argument that a wrapper comparing equal to a
    /// primitive everywhere except where it did not is hard to find. The
    /// argument was against an *implicit* wrapper and does not apply here — the
    /// program wrote `new`. What the divergence actually bought was
    /// `new Number(5).valueOf()` answering `NaN`, in fifteen suite assertions.
    #[construct]
    fn convert(this: u64, value: u64) -> u64 {
        let number = match written(value) {
            // `Number()` is `0`, not `NaN`. The argument being left out is not
            // the same as `undefined` being passed, and this is the one place
            // the difference is visible — see [`written`] for who answers it.
            false => 0.0,
            // A bigint is the one argument `ToNumber` refuses and `Number(x)`
            // accepts: `1n + 1` is a `TypeError` and `Number(1n)` is `1`. So it
            // is answered here rather than in the shared conversion, which every
            // arithmetic operator also reaches.
            true => match with_current(|context| bigint_class::as_f64(context, value)) {
                Some(number) => number,
                // Outside any borrow, because it may run a `valueOf`.
                None => class_support::to_number(value),
            },
        };
        let bits = Value::from_f64(number).bits();
        with_current(|context| primitive_proto::wrap(context, this, bits));
        bits
    }

    /// `n.toString(radix)`.
    ///
    /// Base ten is the shortest round-tripping decimal, which is a different
    /// problem from writing digits in a base and is why the two are separate
    /// paths here rather than one loop with a special case.
    fn to_string(this: u64, radix: u64) -> u64 {
        let base = parse::radix_argument(radix);
        let number = receiver_number(this);
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
            let text = match base {
                None | Some(10) => crate::coerce::number_to_string(number),
                Some(base) => Str::from_str(&format::in_radix(number, base as u32)),
            };
            context.intern_value(text).bits()
        })
    }

    /// `n.valueOf()` — the number itself.
    fn value_of(this: u64) -> f64 {
        receiver_number(this)
    }

    /// `n.toFixed(digits)`.
    ///
    /// Rounds **half away from zero**, which is what the specification says and
    /// what Rust's own `{:.*}` does not: `{:.0}` of `2.5` is `2`, because Rust
    /// formats to nearest-even. `(2.5).toFixed(0)` is `"3"`.
    fn to_fixed(this: u64, digits: f64) -> u64 {
        let number = receiver_number(this);
        let asked = match digits.is_nan() {
            true => 0.0,
            false => digits.trunc(),
        };
        // Grampear era responder `"0.00"` a `(1).toFixed(-1)` e `100` casas a
        // `(1).toFixed(101)` — dois pedidos ilegais atendidos com um numero que
        // o programa nao pediu. O intervalo e da especificacao, e o `RangeError`
        // sai FORA do emprestimo porque construir o erro toma o contexto.
        let Some(places) = in_range(
            asked,
            0.0,
            100.0,
            "toFixed() digits argument must be between 0 and 100",
        ) else {
            return with_current(|context| undefined_of(context));
        };
        with_current(|context| {
            let text = match number.is_finite() && number.abs() < 1e21 {
                true => Str::from_str(&format::fixed(number, places)),
                // Past 1e21 the specification falls back to the ordinary
                // `ToString`, which is why this is not a formatting width but a
                // branch.
                false => crate::coerce::number_to_string(number),
            };
            context.intern_value(text).bits()
        })
    }

    /// `n.toExponential(digits)`.
    ///
    /// The argument left out is not the same as zero digits: `(12).toExponential()`
    /// is `"1.2e+1"` and `(12).toExponential(0)` is `"1e+1"`. So it arrives as
    /// the value it was passed rather than as an `f64`.
    fn to_exponential(this: u64, digits: u64) -> u64 {
        let number = receiver_number(this);
        // Um numero nao finito responde o `ToString` dele ANTES de a faixa ser
        // olhada, que e a ordem da especificacao: `(NaN).toExponential(500)` e
        // `"NaN"` e nao um `RangeError`.
        let places = match (number.is_finite(), places_of(digits)) {
            (true, Some(asked)) => {
                let Some(places) = in_range(
                    asked,
                    0.0,
                    100.0,
                    "toExponential() argument must be between 0 and 100",
                ) else {
                    return with_current(|context| undefined_of(context));
                };
                Some(places)
            }
            _ => None,
        };
        with_current(|context| {
            let text = Str::from_str(&format::exponential(number, places));
            context.intern_value(text).bits()
        })
    }

    /// `n.toPrecision(digits)`.
    ///
    /// With no argument it is `toString`, which the specification states and
    /// which is not the same as one significant digit.
    fn to_precision(this: u64, digits: u64) -> u64 {
        let number = receiver_number(this);
        // Asked BEFORE the borrow: `places_of` takes one of its own, and
        // nesting them aborts the process rather than failing a test — the trap
        // `parse::radix_argument` records paying for once already.
        let asked = places_of(digits);
        // Sem argumento e `toString`, e um numero nao finito tambem — as duas
        // saidas que a especificacao toma antes de olhar para a faixa.
        let places = match (asked, number.is_finite()) {
            (Some(asked), true) => {
                let Some(places) = in_range(
                    asked,
                    1.0,
                    100.0,
                    "toPrecision() argument must be between 1 and 100",
                ) else {
                    return with_current(|context| undefined_of(context));
                };
                Some(places)
            }
            _ => None,
        };
        with_current(|context| {
            let text = match places {
                Some(places) => Str::from_str(&format::precision(number, places)),
                None => crate::coerce::number_to_string(number),
            };
            context.intern_value(text).bits()
        })
    }

    /// `n.toLocaleString()`.
    ///
    /// The plain decimal form, with no grouping. This crate carries no locale
    /// data — the same wall `normalize` and `localeCompare` stop at — and
    /// inventing one locale's separators would make the answer wrong for every
    /// program running under another. `"1,234"` is a claim about the reader, not
    /// about the number.
    fn to_locale_string(this: u64) -> u64 {
        let number = receiver_number(this);
        with_current(|context| {
            context
                .intern_value(crate::coerce::number_to_string(number))
                .bits()
        })
    }

    /// `Number.isNaN(x)` — without converting. See the module documentation.
    #[stat]
    #[js("isNaN")]
    fn is_nan(value: u64) -> bool {
        as_double(value).is_some_and(f64::is_nan)
    }

    /// `Number.isFinite(x)` — without converting.
    #[stat]
    fn is_finite(value: u64) -> bool {
        as_double(value).is_some_and(f64::is_finite)
    }

    /// `Number.isInteger(x)`.
    #[stat]
    fn is_integer(value: u64) -> bool {
        as_double(value).is_some_and(|number| number.is_finite() && number.fract() == 0.0)
    }

    /// `Number.isSafeInteger(x)`.
    #[stat]
    fn is_safe_integer(value: u64) -> bool {
        as_double(value).is_some_and(|number| {
            number.is_finite() && number.fract() == 0.0 && number.abs() <= 9_007_199_254_740_991.0
        })
    }
}

/// `Boolean`.
#[rtse::class("Boolean")]
impl Boolean {
    /// `Boolean(x)` — `ToBoolean` of an argument.
    ///
    /// `new Boolean(x)` answers the object, which remembers the flag for the
    /// reason [`Number::convert`] records — and the wrapper is where the flag
    /// mattered most: an object is truthy, so `new Boolean(false)` read as
    /// `true` in every one of the eight assertions that asked.
    #[construct]
    fn convert(this: u64, value: u64) -> bool {
        let flag = class_support::to_boolean(value);
        let bits = Value::from_bool(flag).bits();
        with_current(|context| primitive_proto::wrap(context, this, bits));
        flag
    }

    /// `b.toString()` — `"true"` or `"false"`.
    ///
    /// Reached because [`crate::entry::objects::get_property`] starts a read on
    /// a boolean here. Without it `true.toString()` was `undefined`, which is a
    /// method a program can see is missing rather than one it never looks for.
    fn to_string(this: u64) -> u64 {
        let text = match receiver_boolean(this) {
            true => "true",
            false => "false",
        };
        with_current(|context| context.intern_value(Str::from_str(text)).bits())
    }

    /// `b.valueOf()`.
    fn value_of(this: u64) -> bool {
        receiver_boolean(this)
    }
}

/// Whether the call that reached `Number` actually WROTE an argument.
///
/// `Number()` is `+0` and `Number(undefined)` is `NaN` — the one place the
/// language makes "left out" and "`undefined` was passed" different answers.
/// The calling convention pads its four slots with `undefined`, so the value
/// alone cannot say which happened; the call SITE can, and
/// [`crate::entry::functions::called`] records the count it wrote for exactly
/// this kind of question.
///
/// Where nobody said, the older reading stands: `undefined` is taken as the
/// padding, so `new Number()` is still `0`. That leaves a stated divergence —
/// `[undefined].map(Number)` is `[NaN]` in Node and `0` here, because a native
/// calling another function honestly does not know a count — and narrowing it
/// needs the count at `functions::call`, which only a compiled site has.
fn written(value: u64) -> bool {
    with_current(
        |context| match context.pending_counts.last().copied().flatten() {
            Some(count) => count > 0,
            None => value != undefined_of(context),
        },
    )
}

/// The number a `Number.prototype` method's receiver IS.
///
/// `thisNumberValue`, in the two spellings a receiver has: the primitive
/// itself, or the wrapper object holding it. Never a conversion of an arbitrary
/// object — `class_support::this_number` records why, and the reason is
/// mechanical: converting a receiver looks up `valueOf` and calls it, which is
/// the very body this feeds, and that recursed until the stack ran out.
///
/// The unwrap is here rather than inside `this_number` because `this_number` is
/// also what `String.prototype`'s numeric receivers go through, and widening it
/// would make one function answer for two `[[Data]]` slots.
fn receiver_number(this: u64) -> f64 {
    class_support::this_number(primitive_proto::unwrapped(this))
}

/// The flag a `Boolean.prototype` method's receiver IS.
///
/// `thisBooleanValue`, and the unwrap is the whole method: an object is truthy,
/// so `new Boolean(false).valueOf()` was `true` without it — a wrong answer that
/// looks exactly like a right one.
fn receiver_boolean(this: u64) -> bool {
    class_support::to_boolean(primitive_proto::unwrapped(this))
}

/// A digit count an argument names, with `None` for "not given".
///
/// Its own function because two methods need the distinction and each answers
/// something different without it — `toExponential` shortens and `toPrecision`
/// becomes `toString`.
fn places_of(digits: u64) -> Option<f64> {
    let absent = with_current(|context| undefined_of(context));
    if digits == absent {
        return None;
    }
    let asked = class_support::to_number(digits);
    match asked.is_nan() {
        true => Some(0.0),
        false => Some(asked.trunc()),
    }
}

/// The digit count, when the request is inside the range the specification
/// allows — and a `RangeError` with `None` when it is not.
///
/// Grampear em vez de recusar era a decisao anterior, e ela transformava
/// `(1).toPrecision(0)` — que a linguagem recusa — num `"1"` que o programa nao
/// pediu. A excecao e levantada FORA de qualquer emprestimo: construir o objeto
/// de erro toma o contexto, que e o que `string::basic` ja documenta.
fn in_range(asked: f64, low: f64, high: f64, message: &str) -> Option<usize> {
    if !(low..=high).contains(&asked) {
        throw::range_error(message);
        return None;
    }
    Some(asked as usize)
}

/// The double a value holds, when it genuinely holds one.
///
/// `None` for everything else, which is what makes `Number.isNaN` answer false
/// for a string rather than converting it.
fn as_double(value: u64) -> Option<f64> {
    Value(value).as_f64()
}
