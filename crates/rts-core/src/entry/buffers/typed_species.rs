//! `t.slice(begin, end)` — the copy, and which class it belongs to.
//!
//! # Why the answer's class is a question at all
//!
//! `slice` is the one member of the family that makes a **new** object, so it
//! is the one that has to decide what that object is. The language decides it
//! with the species protocol: the receiver's `constructor`, then that
//! constructor's `Symbol.species`, and what comes back is what gets
//! constructed. A subclass therefore controls what its own `slice` answers —
//! which is the whole reason the protocol exists, and is observable as
//! `y.constructor.name` and `y.BYTES_PER_ELEMENT`.
//!
//! # Why the ordinary copy is not routed through it
//!
//! Because the protocol is two property reads and a constructor call, all three
//! of which are user code, and almost every program's answer is "the class it
//! already is". So an absent species takes the path this method has always
//! taken — a fresh buffer and one byte copy — and the protocol only decides
//! anything for a program that asked it to.
//!
//! That is not only a cost argument. The byte copy is **exact**: a `Float32`
//! `NaN` keeps its payload, where an element-by-element copy reads each element
//! as a double and writes it back through a narrowing conversion. The
//! specification makes the same split for the same reason, taking the bytes
//! when the two classes are the same type and going element by element only
//! when they differ.
//!
//! # Why a mismatched content type copies nothing rather than throwing
//!
//! A species answering `BigInt64Array` for a slice of a `Uint8Array` is a
//! `TypeError` in the language. [`super`] states this module's answer to that
//! refusal — the write is dropped and the element keeps what it held — and this
//! follows it rather than raising, so that one rule about mixing bigint and
//! numeric elements has one answer everywhere in the module.

use super::{View, typed, with_current};
use crate::entry::array_proto::species::property;
use crate::value::Value;

/// `t.slice(begin, end)` — a copy, in an object the species protocol decides.
pub(in crate::entry) fn slice(this: u64, begin: u64, end: u64) -> u64 {
    // Before any borrow: each of these takes one of its own.
    let first = super::optional_number(begin);
    let last = super::optional_number(end);
    let Some(count) = with_current(|context| {
        let view = super::view_of(context, this)?;
        let (first, last) = super::range(view.count(), first, last);
        Some(last - first)
    }) else {
        return super::undefined();
    };

    // Outside every borrow: reading `constructor` may run a getter and
    // constructing runs a constructor, and both are user code that calls
    // straight back into the runtime.
    let made = species_made(this, count);
    if crate::entry::throw::in_flight() {
        // The protocol threw. Answering the default copy would hide it behind a
        // value the program never asked for — the compiled call site above
        // re-raises what is in flight, so this only has to stop.
        return super::undefined();
    }
    match made {
        Some(made) => filled(this, made, begin, end),
        None => copied(this, begin, end),
    }
}

/// The species-constructed destination, if the answer is not this class itself.
///
/// # Why an absent `Symbol.species` is the CONSTRUCTOR and not nothing
///
/// Because the shared `%TypedArray%` prototype defines that species getter as
/// `return this`, so a subclass that overrides nothing still slices into
/// **itself**: `class Same extends Uint8Array {}` answers a `Same`, not a
/// `Uint8Array`. There is no `%TypedArray%` object here to hang that getter on
/// — [`super::typed_classes`] records why — so the rule it states is answered
/// directly instead of being read off an object that does not exist.
///
/// `None` is the ordinary case, and it is narrower than "nothing was found": it
/// means the species IS the built-in class for this view's kind, which the
/// default copy already produces, exactly and without running a constructor.
/// The other three answers folded into it are a receiver with no
/// `constructor`, a species that is `undefined` or `null` — the two the
/// specification names — and one that is not something to construct with, where
/// the language throws a `TypeError` this layer cannot raise where a handler
/// could catch it.
///
/// The two property reads themselves are [`crate::entry::array_proto::species`]'s,
/// which is where the copy that used to live here went: the rule that a species
/// getter runs OUTSIDE the borrow that found it is one rule, and two spellings
/// of it is one of them eventually running inside. What stays here is the part
/// that genuinely differs — an absent species means the receiver's own class,
/// where for an array it means `Array`.
fn species_made(this: u64, count: usize) -> Option<u64> {
    let constructor = property(this, "constructor")?;
    // `"@@species"` is the key text a `static get [Symbol.species]()` member
    // writes to — see `entry::symbol` for why a symbol key is a name in a space
    // no program can spell. Reached by that spelling rather than by minting the
    // symbol and asking for its key, because minting takes a borrow to answer
    // what this string already says.
    let answered = property(constructor, "@@species").unwrap_or(constructor);
    let species = with_current(|context| {
        let cell = Value(answered).as_slot()?;
        context.callable_at(cell)?;
        // The built-in for this kind answers what the default copy answers, so
        // it is not worth a construction — and going through one would lose the
        // byte copy's exactness for no observable difference.
        let kind = super::view_of(context, this)?.kind;
        let built_in = super::typed_classes::named(kind)
            .and_then(|name| crate::entry::class_support::made(context, name));
        match built_in == Some(answered) {
            true => None,
            false => Some(answered),
        }
    })?;
    let absent = super::undefined();
    let length = Value::from_f64(count as f64).bits();
    Some(crate::entry::functions::construct(
        species, length, absent, absent, absent,
    ))
}

/// The elements copied into an object the species protocol made.
///
/// Through [`typed::subarray`] and [`typed::copy_from`] rather than a loop of
/// its own: `A.set(O.subarray(begin, end))` is exactly what an element-by-
/// element species copy IS, and both halves already convert between kinds in
/// the one place the module keeps that decision. The window object it makes is
/// the cost, and it is paid only by a program that named a species.
///
/// The made object being something other than a view is left alone rather than
/// filled: a species may answer any constructor, and writing into whatever came
/// back is not a copy, it is a guess.
fn filled(this: u64, made: u64, begin: u64, end: u64) -> u64 {
    let window = typed::subarray(this, begin, end);
    typed::copy_from(made, window, super::undefined());
    made
}

/// The ordinary copy: the same elements, in a buffer of the answer's own.
///
/// Byte for byte, which is what keeps it exact — see the module documentation
/// for why that matters for a float.
fn copied(this: u64, begin: u64, end: u64) -> u64 {
    let begin = super::optional_number(begin);
    let end = super::optional_number(end);
    with_current(|context| {
        let absent = crate::entry::objects::undefined_of(context);
        let Some(view) = super::view_of(context, this) else {
            return absent;
        };
        let (first, last) = super::range(view.count(), begin, end);
        let size = view.kind.size();
        let Some(bytes) = super::window(context, &view) else {
            return absent;
        };
        // Copied out before the buffer is made, because allocating one takes
        // the byte store mutably and this slice borrows it.
        let taken = bytes[first * size..last * size].to_vec();
        let Some(buffer) = super::new_buffer(context, taken.len()) else {
            return absent;
        };
        let fresh = View {
            buffer,
            offset: 0,
            length: taken.len(),
            kind: view.kind,
        };
        if let Some(destination) = super::window_mut(context, &fresh) {
            destination.copy_from_slice(&taken);
        }
        typed::made(context, fresh)
    })
}
