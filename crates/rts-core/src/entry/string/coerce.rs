//! The conversions a string method runs BEFORE it takes the context borrow.
//!
//! # Why they are a module rather than lines in `mod.rs`
//!
//! Because they share one constraint, and it is the constraint the rest of the
//! folder is arranged around: each of them may CALL USER CODE. `ToString` of an
//! object runs a `toString`, `ToNumber` runs a `valueOf`, and the brand test
//! reads a property that may be a getter — so none of them can happen inside
//! `with_current`, where every other helper in this folder lives. Keeping them
//! together is what makes "this one is outside the borrow" a property of the
//! file rather than a comment repeated five times.
//!
//! Each answers `Option`, and `None` always means the same thing: a throw is
//! recorded and the method must answer nothing. That is rule 8 of this crate's
//! README seen from the raising side — the type asks the question so a caller
//! cannot inherit a wrong answer by forgetting to.

use super::super::objects::undefined_of;
use super::super::with_current;
use super::receiver;
use crate::value::Value;

/// `this` as a STRING, running the conversion the language runs.
///
/// # What this closes
///
/// Every method in this folder read its receiver through [`receiver`], which
/// answers text for a primitive and for a `new String(…)` wrapper and answers
/// the value back for anything else — so `units_of` found no text and the
/// method answered `undefined`. That is not a corner: `String.prototype.trim`
/// is specified over `ToString(RequireObjectCoercible(this))`, so
/// `[].join === Array.prototype.join` is the *unusual* case and a method
/// borrowed onto a number, an array or a plain object is ordinary JavaScript.
/// `string/claude-string-methods-generic-this` measured seventeen rows of
/// `undefined` where Node and Bun answer, `(42.5).toString`-style receivers
/// included.
///
/// # Why it is a prologue rather than something [`receiver`] does
///
/// Because it CALLS. `ToString` of an object runs `toString`/`valueOf`, and
/// every caller of `receiver` holds the context borrow — running user code
/// under one is the nested borrow that aborts the process rather than failing.
/// So the conversion happens once, at the top of the native, before any borrow
/// is taken, and what reaches the borrow is a text cell that `receiver` passes
/// straight through.
///
/// `None` means the method must answer nothing: either `RequireObjectCoercible`
/// refused `null`/`undefined`, or the conversion threw. Both leave a throw
/// recorded, which is what the caller's `nothing(context)` is there to sit
/// beside — rule 8 of this crate's README, from the raising side.
pub(super) fn coerce_receiver(this: u64) -> Option<u64> {
    // Already text, primitive or wrapped: the common case, and it must not pay
    // for a `ToPrimitive` that would re-enter the property path per call.
    let held = with_current(|context| {
        let held = receiver(context, this);
        Value(held)
            .as_slot()
            .filter(|cell| context.text_at(*cell).is_some())
            .map(|_| held)
    });
    if held.is_some() {
        return held;
    }

    // `RequireObjectCoercible`, which is a `TypeError` and not a conversion:
    // `String.prototype.trim.call(null)` throws where `String(null)` answers
    // `"null"`, and collapsing the two would make the method answer text for a
    // receiver the language refuses.
    let nullish = with_current(|context| match Value(this).kind() {
        crate::value::Kind::Singleton(number) => {
            number == context.singletons.undefined || number == context.singletons.null
        }
        _ => false,
    });
    if nullish {
        super::super::throw::type_error(
            "String.prototype method called on null or undefined",
        );
        return None;
    }

    let primitive = super::super::primitive::to_primitive(this, crate::coerce::Hint::String);
    if super::super::throw::in_flight() {
        return None;
    }
    // A symbol is the one primitive `ToString` refuses, and it must refuse it
    // HERE too — `super::super::symbol::described` is the exception `String(sym)` is,
    // and a method receiver is not that exception.
    if with_current(|context| super::super::symbol::described(context, primitive).is_some()) {
        super::super::throw::type_error("Cannot convert a Symbol value to a string");
        return None;
    }
    let text = with_current(|context| {
        super::super::text::to_text(context, Value(primitive)).map(|text| context.intern_value(text).bits())
    });
    if text.is_none() {
        super::super::throw::type_error("Cannot convert object to primitive value");
    }
    text
}

/// `IsRegExp(value)` — the brand `includes`, `startsWith` and `endsWith` refuse.
///
/// # Why those three refuse it at all
///
/// Because they do NOT consult a pattern protocol, and a program that passes a
/// regular expression to one of them has almost certainly meant `match` or
/// `search`. The specification makes that a `TypeError` rather than letting it
/// stringify — which is what happened here, silently, and the stringification
/// was itself broken: `String(/b/)` answered `[object RegExp]`, so
/// `"a/b/c".startsWith(/b/)` compared against the wrong text as well.
///
/// # Why `Symbol.match` and not "is it a regular expression"
///
/// The brand is a PROPERTY, so an ordinary object claiming it is refused and a
/// real regular expression denying it is accepted. That is what
/// `symbol/claude2-symbol-match-as-regexp-brand` measures, and it is the reason
/// the test cannot be `regexp_at`: the internal slot is only the fallback, for
/// an object that says nothing.
pub(super) fn is_regexp(value: u64) -> bool {
    if !with_current(|context| super::super::primitive::is_object_in(context, value)) {
        return false;
    }
    let key = with_current(|context| super::super::symbol::well_known(context, "match"));
    let hook = super::super::computed::get_indexed(value, key);
    let absent = with_current(|context| hook == undefined_of(context));
    if !absent {
        return super::super::primitives::to_boolean(hook);
    }
    with_current(|context| {
        Value(value)
            .as_slot()
            .is_some_and(|cell| context.regexp_at(cell).is_some())
    })
}

/// An argument the language reads as TEXT, converted before any borrow.
///
/// [`text_of`] is the half that runs inside one, and its own doc names the same
/// boundary: an object whose `toString` is user code cannot be converted there.
/// `String.raw` is the caller that made it visible — a substitution that was an
/// object was DROPPED, so `` String.raw`a${{}}b` `` lost `[object Object]`
/// entirely rather than answering it.
///
/// Unlike [`coerce_receiver`] this does NOT refuse `null`/`undefined`: they are
/// `"null"` and `"undefined"`, which is what `ToString` says and what a
/// substitution is. A symbol is still refused, because `ToString` refuses it.
pub(super) fn text_arg(value: u64) -> Option<u64> {
    // Already a bare text cell: `ToString(x)` is `x`, and answering so here is
    // the short-circuit [`coerce_receiver`] has had all along and this did not.
    // Without it, `"abcdef".indexOf("cd")` CLONED `"cd"`'s buffer — `to_text`
    // below ends in `.cloned()` — and built a fresh region cell for it, on every
    // call, to answer with a string the caller already handed in.
    //
    // Measured before this existed: `indexOf` on a 16-character string cost
    // 299 ns against `charCodeAt`'s 89 on the same run, and ~210 ns of the
    // difference was this conversion rather than the search — which is why the
    // same call on a 256-character string cost only 22 ns more.
    //
    // Sound because a JavaScript string is immutable and has no identity a
    // program can observe, which `Context::intern_value` states from the other
    // direction. `text_at` compares the cell's TYPE against `text_type`, so a
    // `new String("x")` wrapper is not one of these — it is an ordinary object
    // carrying a `boxed` entry, and it still takes the path below, because its
    // `toString` may be user code and the language says that runs.
    let bare = with_current(|context| {
        Value(value)
            .as_slot()
            .filter(|cell| context.text_at(*cell).is_some())
            .map(|_| value)
    });
    if bare.is_some() {
        return bare;
    }

    let primitive = super::super::primitive::to_primitive(value, crate::coerce::Hint::String);
    if super::super::throw::in_flight() {
        return None;
    }
    if with_current(|context| super::super::symbol::described(context, primitive).is_some()) {
        super::super::throw::type_error("Cannot convert a Symbol value to a string");
        return None;
    }
    let text = with_current(|context| {
        super::super::text::to_text(context, Value(primitive)).map(|text| context.intern_value(text).bits())
    });
    if text.is_none() {
        super::super::throw::type_error("Cannot convert object to primitive value");
    }
    text
}

/// An argument the language reads as a NUMBER, converted before any borrow.
///
/// [`integer_arg`] is the half that runs inside one, and its own doc names the
/// gap this closes: an object answered `NaN` there, because `ToNumber` on one
/// runs user code. So `"ab".repeat({ valueOf: () => 2 })` was the empty string
/// and `"abc".substring({valueOf:()=>1}, {valueOf:()=>3})` was the whole string.
///
/// A **symbol** and a **BigInt** are the two primitives `ToNumber` refuses, and
/// refusing them is not decoration: `"ab".repeat(2n)` answered `""` where every
/// engine throws, which is a wrong answer that runs. `None` says a throw was
/// recorded and the method must answer nothing.
pub(super) fn number_arg(value: u64) -> Option<u64> {
    // These values are already primitives and `ToPrimitive` cannot change them
    // or run user code. Returning the original word avoids the generic crossing
    // on integer indices such as `s.charCodeAt(i & 15)`.
    if matches!(
        Value(value).kind(),
        crate::value::Kind::Float
            | crate::value::Kind::Int
            | crate::value::Kind::Bool
            | crate::value::Kind::Singleton(_)
    ) {
        return Some(value);
    }
    let primitive = super::super::primitive::to_primitive(value, crate::coerce::Hint::Number);
    if super::super::throw::in_flight() {
        return None;
    }
    let refused = with_current(|context| {
        if super::super::symbol::described(context, primitive).is_some() {
            return Some("Cannot convert a Symbol value to a number");
        }
        match super::super::bigints::digits_of(context, primitive) {
            Some(_) => Some("Cannot convert a BigInt value to a number"),
            None => None,
        }
    });
    if let Some(message) = refused {
        super::super::throw::type_error(message);
        return None;
    }
    Some(primitive)
}
