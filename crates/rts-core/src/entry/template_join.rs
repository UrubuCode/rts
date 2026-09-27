//! `` `a${x}b${y}c` `` — every piece and every value joined, in ONE crossing.
//!
//! It was a chain of `+`. Three pieces and two values is four additions, and
//! each one allocates a string that the next addition immediately makes
//! garbage — so a template built N intermediate strings to answer with one.
//! Measured: a template cost ~940 ns an evaluation, against ~200 for the
//! string methods beside it.
//!
//! `which` is the template SITE, whose literal pieces were declared when the
//! program was placed — the same numbering `template_strings` reads for a
//! tagged template. So the pieces cost a lookup and no allocation at all, and
//! only the values are coerced.
//!
//! # Why six values, and why the count is a shared constant
//!
//! The arguments are scalars across an `extern "C"` boundary, so the count is
//! fixed at the signature. It was three, and `bench/analytic.ts` `template 4
//! holes` paid the chain at 1 208 ns while two holes cost 327. `ArrayOf` already
//! carries eight the same way, so six here decides nothing new about the
//! convention. The compiler pads the slots it does not use and the runtime reads
//! only `count`; [`TEMPLATE_JOINED`] is the number both sides state and
//! `rts-host` asserts they agree, as it does for `ARRAY_OF_SLOTS`.
//!
//! # Why the primitive case allocates nothing but the answer
//!
//! This took a `Rooted` list, a `Vec` of converted texts and a `Vec` of bytes
//! for EVERY template — three trips to the process allocator, at about 30 ns
//! each here, for a join whose answer is usually under thirty bytes and lives
//! inline in its `Str`. `examples/string_cost` put the whole join at 246 ns of
//! which the number's own text was 16.
//!
//! A `Rooted` protects values a native holds while USER CODE may allocate, and
//! the only user code a template can run is an object's `toString`. So the
//! values are inspected first: where none is an object no hook runs, nothing
//! allocates before the final string owns its bytes, and the values sit in a
//! stack array the conservative scan already sees. Where one IS an object the
//! path below is the old one, `Rooted` included, and it is rare.
//!
//! The converted texts go in a fixed array, and the bytes are assembled on the
//! stack when the answer fits inline — the copy into the `Str` is the same copy
//! `Narrow::from_slice` always made, so a buffer on the heap was paying to be
//! freed.

use super::objects::undefined_of;
use super::text::to_text;
use super::{Context, with_current};
use crate::text::{INLINE, Str};
use crate::value::{Kind, Value};

/// How many substitutions [`template_join`] takes in one crossing.
///
/// The compiler states the same number as `rts_codegen::runtime::TEMPLATE_JOINED`
/// and `rts-host` refuses to compile when the two disagree.
pub const TEMPLATE_JOINED: usize = 6;

/// A template substitution that either borrows an already-rooted text cell or
/// owns a primitive spelling produced without allocating an intermediate cell.
///
/// Keeping the two distinct avoids cloning an existing string and avoids
/// interning a number only to read it back immediately.
enum TemplateText {
    /// A primitive string cell, kept alive by the caller's arguments or by the
    /// rooted list of the object path.
    Borrowed(u64),
    /// Text produced directly from a primitive value.
    Owned(Str),
}

impl TemplateText {
    /// The text represented by this substitution while `context` is borrowed.
    fn as_str<'a>(&'a self, context: &'a Context) -> Option<&'a Str> {
        match self {
            Self::Borrowed(value) => Value(*value)
                .as_slot()
                .and_then(|cell| context.text_at(cell)),
            Self::Owned(text) => Some(text),
        }
    }
}

/// `` `a${x}b${y}c` `` — every piece and every value joined, in ONE crossing.
///
/// Literal pieces are already registered at compile time. Substitutions are
/// converted with the string hint — an object's `toString` runs OUTSIDE the
/// borrow, and only then — and assembled into one final allocation.
#[rtse::entry]
pub fn template_join(
    which: i64,
    count: i64,
    v0: u64,
    v1: u64,
    v2: u64,
    v3: u64,
    v4: u64,
    v5: u64,
) -> u64 {
    let wanted = count.clamp(0, TEMPLATE_JOINED as i64) as usize;
    let mut values = [v0, v1, v2, v3, v4, v5];
    let values = &mut values[..wanted];

    let (any_object, symbol) = with_current(|context| {
        (
            values.iter().any(|&value| super::primitive::is_object_in(context, value)),
            context.kinds.symbol,
        )
    });
    // Rule 8 from the raising side: `to_text` answers `None` for a symbol, and
    // the join used to SKIP it — `` `${Symbol()}` `` answered `""` where the
    // language raises. Asked before the borrow, because building the error
    // borrows the context.
    if values
        .iter()
        .any(|&value| matches!(Value(value).kind(), Kind::Client { tag, .. } if tag == symbol))
    {
        super::throw::type_error("Cannot convert a Symbol value to a string");
        return with_current(|context| undefined_of(context));
    }
    if !any_object {
        return with_current(|context| join(context, which, values));
    }

    // An object may execute its own `toString`, and that code can re-enter the
    // runtime. The primitive results stay rooted until the final string owns its
    // bytes, so a hook returning a newly allocated string cannot be swept while
    // another substitution is converted.
    let mut primitives = super::rooted::Rooted::new();
    for &value in values.iter() {
        let primitive = super::primitive::to_primitive(value, crate::coerce::Hint::String);
        if super::throw::in_flight() {
            return with_current(|context| undefined_of(context));
        }
        primitives.values().push(primitive);
    }
    // A hook may answer a symbol too, and the language raises for it the same way.
    if primitives
        .as_slice()
        .iter()
        .any(|&value| matches!(Value(value).kind(), Kind::Client { tag, .. } if tag == symbol))
    {
        super::throw::type_error("Cannot convert a Symbol value to a string");
        return with_current(|context| undefined_of(context));
    }
    let answer = with_current(|context| join(context, which, primitives.as_slice()));
    drop(primitives);
    answer
}

/// The text of literal `piece`, where the table has one.
fn literal_text(context: &Context, piece: u32) -> Option<&Str> {
    context
        .literals
        .get(piece as usize)
        .and_then(|&literal| Value(literal).as_slot())
        .and_then(|cell| context.text_at(cell))
}

/// The pieces of site `which` interleaved with `values`, already primitive, as
/// one new string.
fn join(context: &mut Context, which: i64, values: &[u64]) -> u64 {
    let Some((pieces, _)) = context.templates.get(which as usize) else {
        return undefined_of(context);
    };
    let mut converted: [Option<TemplateText>; TEMPLATE_JOINED] = [const { None }; TEMPLATE_JOINED];
    for (slot, &value) in converted.iter_mut().zip(values) {
        *slot = if Value(value)
            .as_slot()
            .is_some_and(|cell| context.text_at(cell).is_some())
        {
            Some(TemplateText::Borrowed(value))
        } else {
            to_text(context, Value(value)).map(TemplateText::Owned)
        };
    }

    let mut capacity = 0usize;
    let mut narrow = true;
    for text in pieces.iter().filter_map(|&piece| literal_text(context, piece)) {
        capacity += text.len();
        narrow &= text.narrow().is_some();
    }
    for text in converted.iter().flatten().filter_map(|text| text.as_str(context)) {
        capacity += text.len();
        narrow &= text.narrow().is_some();
    }

    if narrow {
        // On the stack when the answer fits inline, which is nearly always: the
        // `Str` copies the bytes in either way, so a heap buffer here was a
        // malloc and a free to hold them for a moment.
        let mut short = [0u8; INLINE];
        let mut long = Vec::new();
        let mut written = 0usize;
        let spill = capacity > INLINE;
        if spill {
            long.reserve_exact(capacity);
        }
        let mut push = |bytes: &[u8]| {
            if spill {
                long.extend_from_slice(bytes);
            } else {
                short[written..written + bytes.len()].copy_from_slice(bytes);
                written += bytes.len();
            }
        };
        for (at, &piece) in pieces.iter().enumerate() {
            if let Some(text) = literal_text(context, piece) {
                push(text.narrow().expect("narrow was proved"));
            }
            if let Some(Some(text)) = converted.get(at)
                && let Some(text) = text.as_str(context)
            {
                push(text.narrow().expect("narrow was proved"));
            }
        }
        let text = if spill {
            Str::owning_latin1(long)
        } else {
            Str::from_latin1(&short[..written])
        };
        return context.intern_value(text).bits();
    }

    let mut units = Vec::with_capacity(capacity);
    for (at, &piece) in pieces.iter().enumerate() {
        if let Some(text) = literal_text(context, piece) {
            units.extend(text.units());
        }
        if let Some(Some(text)) = converted.get(at)
            && let Some(text) = text.as_str(context)
        {
            units.extend(text.units());
        }
    }
    context.intern_value(Str::from_utf16(&units)).bits()
}
