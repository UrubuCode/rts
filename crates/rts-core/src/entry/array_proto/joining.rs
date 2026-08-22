//! Turning an array into text.
//!
//! # Why this left [`super`]
//!
//! Because that file had crossed the 500-line ceiling this crate's README sets,
//! and of everything in it this is the piece that belongs together and belongs
//! nowhere else: the separator, the empty set, and the per-element conversion are
//! one rule, and `toString` is the same rule with the separator fixed. Splitting
//! anywhere else would have separated a method from the mutation it shares a
//! `store` with.

use super::super::objects::undefined_of;
use super::super::string::absent;
use super::super::with_current;
use super::staged;
use crate::text::Str;
use crate::value::Value;

/// `a.join(sep)` — the elements as text, `,` by default.
///
/// `undefined` and `null` join as the empty string, which is the whole reason
/// this does not simply convert every element: `[1, null, 2].join()` is `"1,,2"`
/// and a straightforward `ToString` would produce `"1,null,2"`.
///
/// An element that is an object joins as its `toString`, which is what the
/// language does and what the note here used to name as a divergence: it joined
/// as EMPTY, so `[1, [2, 3]].join("-")` answered `"1-"` and the nested data
/// simply vanished. The reason given was that a conversion is a call and this
/// was inside a borrow — true of the shape it had, and the shape is what
/// changed: the elements are copied out first and every conversion runs with no
/// borrow held.
///
/// A cyclic array still recurses. Named rather than guarded, because the guard
/// belongs with the one `JSON.stringify` keeps and there is one place for it.
pub(super) extern "C" fn join(
    _e: u64,
    this: u64,
    separator: u64,
    _a1: u64,
    _a2: u64,
    _a3: u64,
) -> u64 {
    // Everything the walk needs, taken in one borrow that ends here. The
    // elements are a snapshot on purpose: an element's `toString` can reach the
    // array and change it, and a walk reading through would then be indexing a
    // vector that moved underneath it.
    // The separator converts too, and before the borrow like the elements do.
    // `undefined` passes through unchanged, so the absent case below still sees
    // what it expects.
    let separator = super::super::primitive::to_primitive(separator, crate::coerce::Hint::String);
    let staged = with_current(|context| {
        // Generic over an array-LIKE, the same fallback `at` and `slice` take.
        // The specification defines `join` over `LengthOfArrayLike(ToObject(this))`
        // and never asks whether the receiver is an Array, so
        // `Array.prototype.join.call({length: 2, 0: "a", 1: "b"}, "-")` is
        // `"a-b"` — and it answered `undefined` here for every receiver that was
        // not a real array. That is the spelling `Array.prototype.toString`
        // reaches for a non-array receiver, so the two were wrong together.
        let elements = match staged(context, this) {
            Some((_, elements)) => elements,
            None => super::array_like(context, this)?,
        };
        // O buraco vira `undefined` AQUI, dentro do mesmo empréstimo que leu os
        // elementos. Sem isto ele escapava ao conjunto `empty` de baixo — que
        // tem `undefined` e `null` e não tinha como listar um terceiro word —
        // caía em `to_text`, e `[1,,3].join("-")` respondia `1-null-3`. Passar
        // por `visible` em vez de acrescentar `hole_of` ao conjunto é o que
        // impede que a lista tenha de ser lembrada da próxima vez.
        let elements: Vec<u64> = elements
            .iter()
            .map(|held| super::super::array::visible(context, *held))
            .collect();
        let between = match absent(context, separator) {
            true => Str::from_str(","),
            false => super::super::text::to_text(context, Value(separator))?,
        };
        Some((
            between,
            elements,
            [undefined_of(context), null_of(context)],
        ))
    });
    let Some((between, elements, empty)) = staged else {
        return with_current(|context| undefined_of(context));
    };

    // Assembled as UTF-16 UNITS rather than through Rust strings, and that is a
    // correctness difference rather than a shortcut. `Str::to_rust` refuses
    // text that is not well-formed — deliberately, and every other caller
    // depends on that refusal — so a lone surrogate became `""` here and
    // `["a", hi, lo, "b"].join("")` answered a two-unit string where the
    // language says four. `split("")` already produced the right pieces; it
    // was putting them back that lost them.
    // Written STRAIGHT INTO the answer, one buffer, rather than into a
    // `Vec<Vec<u16>>` that is concatenated afterwards.
    //
    // That shape cost one heap allocation per element — a sixteen-element join
    // made seventeen `Vec`s to produce one string — and bought nothing: the
    // pieces were never inspected, reordered or measured, only appended in the
    // order they were produced. `extend` over `Str::units` appends from the
    // iterator with no intermediate, which is the technique
    // `super::super::json::write` already uses for the same reason.
    //
    // The interleaving is safe for the reason the two-stage shape exists at all:
    // `to_primitive` may run a user `toString`, so it runs OUTSIDE every borrow,
    // and the borrow taken to read the text opens and closes inside one
    // iteration. `joined` is an ordinary Rust `Vec` on the stack frame, not a
    // borrow, so carrying it across those calls is not the re-entrancy this
    // module's neighbours guard against.
    //
    // # What the order guarantees, and what it does not
    //
    // The separator is written for every position after the first, INCLUDING
    // one whose element is `undefined`, `null` or a hole — `[1,,3].join("-")` is
    // `"1--3"`. Emitting it before the element rather than after is what keeps
    // that true without a second pass to trim a trailing one.
    let between: Vec<u16> = between.units().collect();
    let mut joined: Vec<u16> = Vec::new();
    for (at, held) in elements.iter().enumerate() {
        if at > 0 {
            joined.extend_from_slice(&between);
        }
        if empty.contains(held) {
            continue;
        }
        let held = super::super::primitive::to_primitive(*held, crate::coerce::Hint::String);
        with_current(|context| {
            if let Some(text) = super::super::text::to_text(context, Value(held)) {
                joined.extend(text.units());
            }
        });
    }

    with_current(|context| context.intern_value(Str::from_utf16(&joined)).bits())
}

/// The encoded `null`, which [`join`] treats as the empty string.
fn null_of(context: &super::super::Context) -> u64 {
    rts_cranelift::tags::encode(
        rts_cranelift::tags::TAG_SINGLETON,
        u64::from(context.singletons.null),
    )
}
