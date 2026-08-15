//! Strings a program wrote down, and what `typeof` answers.
//!
//! # Why a literal is a number here
//!
//! A string is a heap value, and two occurrences of `"a"` in a program are *the
//! same string* — which is interning, which reads a table. So a literal cannot
//! be an immediate in the compiled code, and what the code carries instead is
//! **which** literal.
//!
//! The text arrives separately: the compiler collects every literal it saw, the
//! host seeds this table with them before the program runs, and the number in
//! the code indexes it. That is the same shape as the property-key numbering and
//! the singleton numbering, and it has the same failure mode — a number that
//! names nothing — handled the same way, by the host seeding from what the
//! compilation actually produced.
//!
//! # Why the values are made once, at seeding
//!
//! A literal evaluated twice must be the same string, and making one per
//! evaluation would allocate on every pass of a loop. So the table holds
//! *values*, interned when it is seeded, and reaching a literal is an index
//! rather than an allocation.

use super::objects::undefined_of;
use super::{Context, with_current};
use crate::text::Str;
use crate::value::{Kind, Value};

/// Seeds the literal table for a program about to run.
///
/// Called by the host with what the compilation collected, in the order it
/// collected them — the index the code carries is a position in that list, so
/// the order is the agreement rather than an incidental detail.
///
/// # Why UTF-16 code units and not `String`
///
/// Because a string literal is not Rust text. `"\uD83D"` is a legal one-unit
/// JavaScript string holding half a surrogate pair, and no `String` can carry
/// that unit — so a table of `String`s meant the compiler had already replaced
/// it with `U+FFFD` by the time this was called, and `isWellFormed()` answered
/// `true` about a string the program never wrote. [`Str::from_utf16`] takes the
/// units for the same reason, and narrows them when they fit.
pub fn declare_literals(context: &mut Context, texts: &[Vec<u16>]) {
    // A loop rather than a `map`, because interning needs the context mutably
    // and so does the field being filled. Pushing one at a time is also what
    // makes the index a position in `texts` rather than something a collect
    // happened to preserve.
    context.literals.clear();
    for text in texts {
        let value = context.intern_value(Str::from_utf16(text)).bits();
        context.literals.push(value);
    }
}

/// `ToString` of a primitive.
///
/// # Why this is shared rather than written where it is needed
///
/// Two operations need it and they look unrelated: `+` converts the non-string
/// side of a concatenation, and a computed property key converts whatever was
/// written between the brackets. Both are `ToString`, and the first version had
/// it inline in `+` — where a second copy for property keys would have been the
/// third statement of the same table.
///
/// An object answers `None` rather than `"[object Object]"`. `ToPrimitive` on
/// one runs a `toString`, which is user code an entry point cannot call, so the
/// absence is a contract violation the caller reports rather than a conversion
/// this can perform.
pub(super) fn to_text(context: &Context, value: Value) -> Option<Str> {
    match value.kind() {
        Kind::Float | Kind::Int => Some(crate::coerce::number_to_string(value.numeric()?)),
        Kind::Bool => Some(Str::from_str(
            if rts_cranelift::tags::payload_of(value.bits()) == rts_cranelift::tags::BOOL_TRUE {
                "true"
            } else {
                "false"
            },
        )),
        Kind::Singleton(number) => Some(Str::from_str(if number == context.singletons.undefined {
            "undefined"
        } else {
            "null"
        })),
        // A string is its own text; anything else on the heap is an object.
        Kind::Reference(slot) => context.text_at(slot as u32).cloned(),
        // A bigint prints its digits. A **symbol does not convert at all** —
        // `"" + sym` is a `TypeError` in the language, deliberately, so that a
        // symbol never becomes text by accident. `None` is the same absence an
        // object gets, and it is the right one: the explicit spelling is
        // `sym.toString()`, which is a method rather than a conversion.
        Kind::Client { tag, payload } if tag == context.kinds.bigint => context
            .bigint_at(payload)
            .map(|held| Str::from_str(&held.to_decimal())),
        Kind::Client { .. } => None,
    }
}

/// `ToString(value)` — the conversion with the **string** hint.
///
/// # Why this is an entry point and not `+` with a literal
///
/// Because the hint is the whole difference. A template substitution is
/// `ToString(value)`, and `+` is `ToPrimitive(value, default)`: an object with
/// both `valueOf` and `toString` answers the SECOND for a template and the
/// FIRST for an addition, and `` `${o}` `` was lowered as an addition — so
/// `{ toString: () => "T", valueOf: () => 42 }` interpolated as `42`. There is
/// no spelling of `+` that fixes it, because the operator's own definition is
/// the wrong one here.
///
/// A symbol still refuses to convert, which is not an omission: the language
/// makes implicit conversion of a symbol a `TypeError` precisely so that one
/// never becomes text by accident, and a template is an implicit conversion.
/// `String(sym)` is the explicit spelling and it is the only one.
#[rtse::entry]
pub fn string_of(value: u64) -> u64 {
    // Outside the borrow, because `toString` is user code.
    let value = super::primitive::to_primitive(value, crate::coerce::Hint::String);
    with_current(|context| match to_text(context, Value(value)) {
        Some(text) => context.intern_value(text).bits(),
        None => undefined_of(context),
    })
}

/// `` `a${x}b${y}c` `` — every piece and every value joined, in ONE crossing.
///
/// It was a chain of `+`. Three pieces and two values is four additions, and
/// each one allocates a string that the next addition immediately makes
/// garbage — so a template built N intermediate strings to answer with one.
/// Measured: a template cost ~940 ns an evaluation, against ~200 for the
/// string methods beside it.
///
/// `which` is the template SITE, whose literal pieces were declared when the
/// program was placed — the same numbering `template_strings` reads for a
/// tagged template. So the pieces cost a lookup and no allocation at all, and
/// only the values are coerced.
///
/// Three values because the arguments are scalars across an `extern "C"`
/// boundary. A template with more keeps the chain of additions, which is
/// correct rather than a gap: it pays what it always paid.
#[rtse::entry]
pub fn template_join(which: i64, count: i64, v0: u64, v1: u64, v2: u64) -> u64 {
    // Convertidos ANTES do empréstimo, e com o hint STRING: `to_text` sozinho
    // não corre `ToPrimitive` nenhum, então um objeto com `toString` não
    // contribuía nada e um com `valueOf` contribuía o número. Enraizados porque
    // cada conversão interna uma string, e internar aloca.
    let wanted = count.clamp(0, 3) as usize;
    let mut converted = super::rooted::Rooted::new();
    for value in [v0, v1, v2].into_iter().take(wanted) {
        let text = string_of(value);
        converted.values().push(text);
    }
    let values = converted.take();
    with_current(|context| {
        let Some((pieces, _)) = context.templates.get(which as usize) else {
            return undefined_of(context);
        };
        let pieces = pieces.clone();

        // One buffer, grown once and written through. The pieces are already
        // interned strings, so what is built here is the JOIN and not a copy
        // of anything that already existed.
        let mut joined = Str::from_str("");
        for (at, piece) in pieces.iter().enumerate() {
            if let Some(&literal) = context.literals.get(*piece as usize)
                && let Some(text) = crate::value::Value(literal)
                    .as_slot()
                    .and_then(|cell| context.text_at(cell))
            {
                joined = joined.concat(text);
            }
            if at < wanted
                && let Some(text) = to_text(context, crate::value::Value(values[at]))
            {
                joined = joined.concat(&text);
            }
        }
        context.intern_value(joined).bits()
    })
}

/// The string a literal number names.
///
/// Answers `undefined` for a number the table does not have.
#[rtse::entry]
pub fn string_const(which: i64) -> u64 {
    with_current(|context| match context.literals.get(which as usize) {
        Some(value) => *value,
        None => undefined_of(context),
    })
}

/// `typeof v`.
///
/// # Why `null` answers `"object"`
///
/// Because that is what JavaScript does. It is a mistake from 1995 that the
/// language cannot take back, and it is written here rather than corrected,
/// because a program asking `typeof` wants what JavaScript does and not what it
/// should have done.
///
/// # Why a function is distinguished by its layout
///
/// A callable is a cell like any other and the tag says only "a reference". So
/// the answer comes from the cell's header, which is the same mechanism a
/// property read uses to find out that a string is not an object — the machine's
/// own answer to a tag space with no room for a kind.
#[rtse::entry]
pub fn type_of(value: u64) -> u64 {
    with_current(|context| {
        // The INDEX into `TYPE_NAMES`, not the text: the string it names is
        // built at most once per run and cached, because building one allocates
        // a cell — see `Context::type_names`.
        let name = match Value(value).kind() {
            Kind::Float | Kind::Int => TypeName::Number,
            Kind::Bool => TypeName::Boolean,
            Kind::Singleton(number) => {
                if number == context.singletons.undefined {
                    TypeName::Undefined
                } else {
                    // `typeof null` is "object".
                    TypeName::Object
                }
            }
            // The two the language declared for itself. Answered from the TAG,
            // with no side table consulted — which is the whole difference
            // between a primitive and a cell wearing a marker, and the reason
            // `typeof` on a symbol used to need a lookup and now does not.
            Kind::Client { tag, .. } if tag == context.kinds.symbol => TypeName::Symbol,
            Kind::Client { tag, .. } if tag == context.kinds.bigint => TypeName::Bigint,
            // A tag the language declared and this was never told about. There
            // is no honest answer, and `"undefined"` is the one that makes a
            // wiring mistake look like a value.
            Kind::Client { .. } => TypeName::Unknown,
            Kind::Reference(slot) => {
                let slot = slot as u32;
                match context.region.type_of(slot) {
                    _ if context.callable_at(slot).is_some() => TypeName::Function,
                    // A string.s cell, which `shape_of` already refuses to
                    // treat as an object for exactly this reason: what a
                    // reference IS is readable from beside the cell rather
                    // than from the encoding.
                    _ if context.text_at(slot).is_some() => TypeName::String,
                    _ => TypeName::Object,
                }
            }
        };
        type_name(context, name)
    })
}

/// Which of [`super::TYPE_NAMES`] an answer is.
///
/// A name rather than an index literal at each arm: the arms and the table are
/// two lists that have to agree, and `TYPE_NAMES[3]` written in a match arm is
/// the spelling where they come to disagree silently.
#[derive(Clone, Copy)]
enum TypeName {
    Number,
    Boolean,
    Undefined,
    Object,
    Symbol,
    Bigint,
    String,
    Function,
    Unknown,
}

/// The string for one of the nine, built on first use and kept for the run.
///
/// See [`super::Context::type_names`] for why this is a cache at all: the
/// answer is one of nine constant words and building one ALLOCATES.
fn type_name(context: &mut Context, name: TypeName) -> u64 {
    let at = name as usize;
    if let Some(held) = context.type_names[at] {
        return held;
    }
    let made = context
        .intern_value(Str::from_str(super::TYPE_NAMES[at]))
        .bits();
    context.type_names[at] = Some(made);
    made
}

/// The text a value has, for a host that has to report a result.
///
/// # Why the host cannot do this itself
///
/// A string is a cell in the region and its bytes are beside it in the slab, so
/// reading one needs the context — which the host installs for the run and takes
/// back afterwards. By the time `run` returns, the only thing left is a word.
///
/// `None` for anything whose conversion runs user code, which is an object: this
/// is a report, and reaching back into the program that produced it to ask what
/// it would like to be called is not what a report should do.
///
/// # Why a lone surrogate is replaced rather than refused
///
/// `Str::to_rust` answers `None` for text that is not valid Unicode, and that
/// absence used to arrive here — where the ONE meaning of `None` is "this is not
/// a primitive". So `console.log("x".concat(halfAnEmoji))` printed an object:
/// the inspector read the absence as "not a string" and dumped its indices.
///
/// A report has no refusal available. Every runtime encodes a lone surrogate as
/// `U+FFFD` on the way to a UTF-8 stream, so that is what this answers —
/// `to_rust_lossy` states the difference between the two conversions and why
/// both are kept.
pub fn described(value: u64) -> Option<String> {
    with_current(|context| Some(to_text(context, Value(value))?.to_rust_lossy()))
}

/// Seeds the property-key numbering from what the compilation resolved.
///
/// # Why the texts and not just how many
///
/// A key the compiler resolved crosses as a number and needs no text: both
/// sides hold the same number. A key the program **computes** does need it —
/// `o[k]` arrives here as a string and has to reach the number the compiler
/// already chose, which a count cannot say.
///
/// Interned in the order given, because interning is what mints the numbers.
/// The compiler orders them by key for exactly that reason, and a different
/// order is a different mapping rather than a cosmetic difference.
pub fn declare_keys(context: &mut Context, texts: &[String]) {
    for text in texts {
        let text = Str::from_str(text);
        context.interner.intern(&text, &mut context.keys);
    }
}

/// Seeds the tagged-template sites for a program about to run.
///
/// Alongside [`declare_literals`] and after it, because a site names its pieces
/// by literal position: seeding these first would record numbers into a table
/// that is about to be cleared.
pub fn declare_templates(context: &mut Context, sites: &[Vec<u32>]) {
    context.templates.clear();
    for pieces in sites {
        context.templates.push((pieces.clone(), None));
    }
}

/// The strings object of a tagged-template site.
///
/// # Why it is built once and kept
///
/// Because the specification says a site has ONE of them for the life of the
/// program: a tag that uses it as a map key must see the same object on every
/// pass, which is the reason tagged templates are used for caching at all.
/// Building it per evaluation is the version that looks identical until a
/// program memoises.
///
/// # Why the pieces are numbers
///
/// They are positions in the literal table, which the compilation filled and the
/// host seeded — so the text crosses once, and a template repeating a piece
/// another literal already spells shares it. The sentinel is a cooked text that
/// does not exist, which is legal only here: a tag reads `raw` instead.
#[rtse::entry]
pub fn template_strings(which: i64) -> u64 {
    let at = which as usize;
    if let Some(made) = with_current(|context| {
        context.templates.get(at).and_then(|(_, made)| *made)
    }) {
        return made;
    }
    let Some(pieces) = with_current(|context| {
        context.templates.get(at).map(|(pieces, _)| pieces.clone())
    }) else {
        return with_current(|context| undefined_of(context));
    };

    // Two arrays and a property, through the ordinary operations rather than by
    // reaching into a layout: what a tag receives has to be an array a program
    // can push to, read a length off, and hand to `Array.from`.
    let cooked = super::array_proto::built(texts(&pieces, 0));
    let raw = super::array_proto::built(texts(&pieces, 1));
    with_current(|context| {
        if let Some(cell) = crate::value::Value(cooked).as_slot() {
            let key = context.well_known("raw");
            super::objects::put(context, cell, key, raw);
        }
        if let Some(site) = context.templates.get_mut(at) {
            site.1 = Some(cooked);
        }
        cooked
    })
}

/// Every cooked or raw piece of a site, as values.
///
/// `step` is which half of each pair to read — 0 for cooked, 1 for raw — which
/// is what keeps one function rather than two that could disagree about the
/// stride they walk.
fn texts(pieces: &[u32], step: usize) -> Vec<u64> {
    with_current(|context| {
        pieces
            .chunks(2)
            .map(|pair| match pair.get(step) {
                // A piece whose escapes were invalid. `undefined` rather than a
                // hole: the array's length is what tells the tag how many
                // pieces there were.
                Some(&NO_COOKED) | None => undefined_of(context),
                Some(&at) => context
                    .literals
                    .get(at as usize)
                    .copied()
                    .unwrap_or_else(|| undefined_of(context)),
            })
            .collect()
    })
}

/// The cooked position of a piece whose escapes are invalid.
///
/// The compiler's `emit::NO_COOKED`, restated because this crate cannot depend
/// on that one — the sentinel is part of the agreement the two sides hold, like
/// the singleton numbering, and the host is where a disagreement would show.
const NO_COOKED: u32 = u32::MAX;
